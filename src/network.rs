// Copyright (c) 2026 sal
// SPDX-License-Identifier: MIT
//! Linux network activity collector for encrypted provider API traffic.
//!
//! Payloads remain encrypted. The collector observes only destination IPs,
//! owning process names, and kernel TCP byte counters exposed by `ss`.

use crate::config::Config;
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    io,
    net::{IpAddr, ToSocketAddrs},
    process::Command,
    time::{Duration, Instant},
};

const CALL_MINIMUM_BYTES: u64 = 256;
const CALL_COOLDOWN: Duration = Duration::from_millis(750);
const TLS_CONNECTION_OVERHEAD_BYTES: u64 = 1_024;
const TOKEN_RATE_WINDOW: Duration = Duration::from_secs(10);
const REQUEST_RATE_WINDOW: Duration = Duration::from_secs(60);

#[derive(Clone, Debug, Default, Serialize, PartialEq)]
pub struct NetworkActivity {
    pub requests_per_minute: f64,
    pub estimated_tokens_per_minute: f64,
    pub active_connections: usize,
    pub detected_calls: u64,
    pub sent_bytes_per_second: f64,
    pub received_bytes_per_second: f64,
    pub source: &'static str,
}

#[derive(Clone, Copy, Debug, Default)]
struct SocketCounters {
    sent: u64,
    received: u64,
}

#[derive(Clone, Debug)]
struct SocketSample {
    key: String,
    peer: IpAddr,
    process: String,
    counters: SocketCounters,
}

#[derive(Default)]
struct ProviderWindow {
    calls: VecDeque<Instant>,
    token_bytes: VecDeque<(Instant, u64)>,
    detected_calls: u64,
}

pub struct NetworkMonitor {
    providers_by_ip: BTreeMap<IpAddr, BTreeSet<String>>,
    previous: BTreeMap<String, SocketCounters>,
    last_calls: BTreeMap<String, Instant>,
    windows: BTreeMap<String, ProviderWindow>,
    bytes_per_token: f64,
    previous_sample: Option<Instant>,
}

impl NetworkMonitor {
    pub fn from_config(config: &Config) -> Self {
        let mut providers_by_ip: BTreeMap<IpAddr, BTreeSet<String>> = BTreeMap::new();
        for (provider, target) in config
            .providers
            .iter()
            .filter(|(_, target)| target.enabled.unwrap_or(true))
        {
            let Some(endpoint) = target.endpoint.as_deref() else {
                continue;
            };
            let Ok(url) = reqwest::Url::parse(endpoint) else {
                continue;
            };
            let Some(host) = url.host_str() else {
                continue;
            };
            let port = url.port_or_known_default().unwrap_or(443);
            if let Ok(addresses) = (host, port).to_socket_addrs() {
                for address in addresses {
                    providers_by_ip
                        .entry(address.ip())
                        .or_default()
                        .insert(provider.clone());
                }
            }
        }
        Self {
            providers_by_ip,
            previous: BTreeMap::new(),
            last_calls: BTreeMap::new(),
            windows: BTreeMap::new(),
            bytes_per_token: config.instruments.network.bytes_per_token,
            previous_sample: None,
        }
    }

    pub fn sample(&mut self) -> io::Result<BTreeMap<String, NetworkActivity>> {
        let output = Command::new("ss")
            .args(["-tinpH", "state", "established"])
            .output()?;
        if !output.status.success() {
            return Err(io::Error::other(
                String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            ));
        }
        Ok(self.observe(&String::from_utf8_lossy(&output.stdout), Instant::now()))
    }

    fn observe(&mut self, output: &str, now: Instant) -> BTreeMap<String, NetworkActivity> {
        let baseline = self.previous_sample.is_none();
        let elapsed = self
            .previous_sample
            .map_or(0.5, |previous| now.duration_since(previous).as_secs_f64())
            .max(0.001);
        self.previous_sample = Some(now);
        let mut received_by_provider: BTreeMap<String, u64> = BTreeMap::new();
        let mut sent_by_provider: BTreeMap<String, u64> = BTreeMap::new();
        let mut connections_by_provider: BTreeMap<String, usize> = BTreeMap::new();
        let mut observed_sockets = BTreeSet::new();

        for socket in parse_ss(output) {
            if socket.process.contains("(\"ingauge\"") {
                continue;
            }
            let Some(providers) = self.providers_by_ip.get(&socket.peer) else {
                continue;
            };
            observed_sockets.insert(socket.key.clone());
            let previous = self.previous.get(&socket.key).copied().unwrap_or_else(|| {
                if baseline {
                    socket.counters
                } else {
                    SocketCounters::default()
                }
            });
            let sent = socket.counters.sent.saturating_sub(previous.sent);
            let received = socket.counters.received.saturating_sub(previous.received);
            let new_socket = !self.previous.contains_key(&socket.key);
            self.previous.insert(socket.key.clone(), socket.counters);
            for provider in providers {
                *connections_by_provider.entry(provider.clone()).or_default() += 1;
                *sent_by_provider.entry(provider.clone()).or_default() += sent;
                *received_by_provider.entry(provider.clone()).or_default() += received;
                let window = self.windows.entry(provider.clone()).or_default();
                let call_ready = self
                    .last_calls
                    .get(&socket.key)
                    .is_none_or(|previous| now.duration_since(*previous) >= CALL_COOLDOWN);
                if sent >= CALL_MINIMUM_BYTES && call_ready {
                    window.calls.push_back(now);
                    window.detected_calls = window.detected_calls.saturating_add(1);
                    self.last_calls.insert(socket.key.clone(), now);
                }
                let attributable_received = if new_socket {
                    received.saturating_sub(TLS_CONNECTION_OVERHEAD_BYTES)
                } else {
                    received
                };
                if attributable_received > 0 {
                    window.token_bytes.push_back((now, attributable_received));
                }
            }
        }
        self.previous
            .retain(|socket, _| observed_sockets.contains(socket));
        self.last_calls
            .retain(|socket, _| observed_sockets.contains(socket));

        let providers = self
            .providers_by_ip
            .values()
            .flatten()
            .cloned()
            .collect::<BTreeSet<_>>();
        providers
            .into_iter()
            .map(|provider| {
                let window = self.windows.entry(provider.clone()).or_default();
                while window
                    .calls
                    .front()
                    .is_some_and(|at| now.duration_since(*at) > REQUEST_RATE_WINDOW)
                {
                    window.calls.pop_front();
                }
                while window
                    .token_bytes
                    .front()
                    .is_some_and(|(at, _)| now.duration_since(*at) > TOKEN_RATE_WINDOW)
                {
                    window.token_bytes.pop_front();
                }
                let token_bytes = window
                    .token_bytes
                    .iter()
                    .map(|(_, bytes)| *bytes)
                    .sum::<u64>();
                (
                    provider.clone(),
                    NetworkActivity {
                        requests_per_minute: window.calls.len() as f64,
                        estimated_tokens_per_minute: token_bytes as f64 / self.bytes_per_token
                            * (60.0 / TOKEN_RATE_WINDOW.as_secs_f64()),
                        active_connections: connections_by_provider
                            .get(&provider)
                            .copied()
                            .unwrap_or(0),
                        detected_calls: window.detected_calls,
                        sent_bytes_per_second: sent_by_provider.get(&provider).copied().unwrap_or(0)
                            as f64
                            / elapsed,
                        received_bytes_per_second: received_by_provider
                            .get(&provider)
                            .copied()
                            .unwrap_or(0) as f64
                            / elapsed,
                        source: "encrypted_network_estimate",
                    },
                )
            })
            .collect()
    }

    #[cfg(test)]
    fn with_target(provider: &str, peer: IpAddr, bytes_per_token: f64) -> Self {
        Self {
            providers_by_ip: BTreeMap::from([(peer, BTreeSet::from([provider.to_owned()]))]),
            previous: BTreeMap::new(),
            last_calls: BTreeMap::new(),
            windows: BTreeMap::new(),
            bytes_per_token,
            previous_sample: None,
        }
    }
}

fn parse_ss(output: &str) -> Vec<SocketSample> {
    let mut samples = Vec::new();
    let mut header: Option<(String, IpAddr, String)> = None;
    for line in output.lines() {
        if line.starts_with(char::is_whitespace) {
            let Some((key, peer, process)) = header.take() else {
                continue;
            };
            let sent = metric(line, "bytes_sent:").unwrap_or(0);
            let received = metric(line, "bytes_received:").unwrap_or(0);
            samples.push(SocketSample {
                key,
                peer,
                process,
                counters: SocketCounters { sent, received },
            });
            continue;
        }
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.len() < 4 {
            header = None;
            continue;
        }
        let Some(peer) = endpoint_ip(fields[3]) else {
            header = None;
            continue;
        };
        let process = fields
            .get(4..)
            .map_or_else(String::new, |parts| parts.join(" "));
        header = Some((
            format!("{}>{}:{}", fields[2], fields[3], process),
            peer,
            process,
        ));
    }
    samples
}

fn endpoint_ip(endpoint: &str) -> Option<IpAddr> {
    if let Some(bracketed) = endpoint.strip_prefix('[') {
        return bracketed
            .split_once(']')
            .and_then(|(ip, _)| ip.parse().ok());
    }
    endpoint.rsplit_once(':')?.0.parse().ok()
}

fn metric(line: &str, prefix: &str) -> Option<u64> {
    line.split_whitespace()
        .find_map(|field| field.strip_prefix(prefix)?.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PEER: &str = "104.18.38.236";

    fn fixture(process: &str, sent: u64, received: u64) -> String {
        format!(
            "0 0 192.168.0.2:40000 {PEER}:443 users:((\"{process}\",pid=7,fd=3))\n\t cubic bytes_sent:{sent} bytes_received:{received}\n"
        )
    }

    #[test]
    fn parses_ipv4_and_ipv6_socket_counters() {
        let parsed = parse_ss(&fixture("worker", 900, 4_000));
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].peer, PEER.parse::<IpAddr>().expect("peer IP"));
        assert_eq!(parsed[0].counters.sent, 900);
        assert_eq!(parsed[0].counters.received, 4_000);
        assert_eq!(
            endpoint_ip("[2a06:98c1:3105::6812:26ec]:443"),
            "2a06:98c1:3105::6812:26ec".parse().ok()
        );
    }

    #[test]
    fn detects_request_and_response_deltas_on_persistent_tls_connections() {
        let peer = PEER.parse().expect("peer IP");
        let mut monitor = NetworkMonitor::with_target("groq", peer, 4.0);
        let start = Instant::now();
        monitor.observe(&fixture("worker", 2_000, 3_000), start);
        let reading = monitor.observe(
            &fixture("worker", 2_800, 7_000),
            start + Duration::from_secs(2),
        );
        let groq = &reading["groq"];
        assert_eq!(groq.requests_per_minute, 1.0);
        assert!(groq.estimated_tokens_per_minute > 0.0);
        assert_eq!(groq.active_connections, 1);
        assert_eq!(groq.source, "encrypted_network_estimate");
    }

    #[test]
    fn excludes_ingauge_provider_polling_from_detected_activity() {
        let peer = PEER.parse().expect("peer IP");
        let mut monitor = NetworkMonitor::with_target("groq", peer, 4.0);
        let reading = monitor.observe(&fixture("ingauge", 8_000, 40_000), Instant::now());
        assert_eq!(reading["groq"].detected_calls, 0);
        assert_eq!(reading["groq"].estimated_tokens_per_minute, 0.0);
    }
}
