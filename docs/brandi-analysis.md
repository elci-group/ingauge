# Brandi coherence analysis

The initial Brandi scan found 516 outward-facing surfaces and scored the repository 66/100 overall: documentation 100, repository documentation 92, and UI strings 74. The two warnings came from the freshly scaffolded default genome naming the wrong product; the remaining informational findings primarily treated internal diagnostics and test assertions as end-user errors.

The implemented brand system defines InGauge as a calm operational instrument: cyan for measurement, green for healthy capacity, amber for constraint, red for critical conditions, and muted blue-grey for unknown state. Human CLI output now shares one renderer with panels, semantic colour, contextual emoji, actionable recovery hints, and a four-frame gauge animation.

After tailoring the genome and presentation layer, strict Brandi lint scores all 548 scanned surfaces at 100/100: documentation 100, repository documentation 100, and UI strings 100, with no findings. The proposal engine still offers cosmetic rewrites for internal test assertions; these were reviewed and rejected because they are not shipped interface copy.

Automation boundaries are deliberate. JSON never receives ANSI, animation, or emoji decoration. Non-interactive output remains ANSI-free. `NO_COLOR`, `CI`, `TERM=dumb`, and `INGAUGE_NO_ANIMATION` suppress motion or colour as appropriate. This preserves scripts, accessibility preferences, logs, and deterministic tests while giving interactive sessions a coherent visual identity.
