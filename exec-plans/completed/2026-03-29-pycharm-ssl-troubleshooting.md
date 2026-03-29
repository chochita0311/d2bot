# PyCharm SSL Troubleshooting

## Metadata

- `status`: `completed`
- `agent`: `main`
- `created`: `2026-03-29`
- `updated`: `2026-03-29`
- `scope`: `Diagnose PyCharm package installation failure caused by missing SSL support in the configured interpreter`
- `related_files`: `README.md`, `.venv/`, `https://www.jetbrains.com/help/pycharm/package-installation-issues.html`

## Request

Investigate the PyCharm package installation error showing that the SSL module is not available for the configured `.venv` interpreter, compare it with the JetBrains guidance, and explain what is happening and what should be done.

## Context

- The repository now documents `.venv` as the canonical local virtual environment.
- The local machine has an Anaconda-based Python 3.7 installation underneath the virtual environment.
- Earlier terminal checks showed that importing `ssl` fails unless the Python DLL and `Library\\bin` paths are added before activating `.venv`.

## Findings

- The configured PyCharm interpreter points at `.venv\Scripts\python.exe`, but that virtual environment is based on `C:\Users\cjsth\Anaconda3\python.exe`.
- In the current local environment, running `python -c "import ssl"` without the helper path setup fails because `_ssl` cannot load.
- Running `python -m pip --version` without the helper path setup resolves to the base Anaconda installation rather than the intended project environment.
- In the old broken environment, a helper script that added DLL paths could temporarily work around the issue in a terminal, but that did not solve the underlying PyCharm interpreter problem.
- This confirmed that the real problem was the interpreter base and not the IDE package action itself.
- The JetBrains troubleshooting guidance confirms the main decision boundary: if the same install attempt fails outside the IDE on the same interpreter, the problem is outside IDE control and must be fixed at the interpreter or environment level.
- The package name `black==23.3.0` is not the primary issue here; the failure happens earlier because TLS/SSL is unavailable in the interpreter process.
- A standalone Python interpreter at `C:\Users\cjsth\AppData\Local\Programs\Python\Python37\python.exe` imports `ssl` correctly and can serve as a clean base interpreter.
- Rebuilding `.venv` from that standalone interpreter fixes the SSL issue and allows normal `pip` network installs to succeed without the old workaround.
- The old broken environment was preserved as `.venv_backup_2026-03-29` during the rebuild.

## Decisions

- Treat this as an interpreter environment issue, not a package metadata issue.
- Recommend a proper Python installation or recreated virtual environment as the clean fix.
- Treat path-hack workarounds as temporary diagnostics only, not the long-term IDE integration solution.
- Recreate `.venv` from the healthy standalone Python interpreter and use that environment in PyCharm going forward.

## Task List

- [x] Create an execution plan for the PyCharm SSL troubleshooting task.
- [x] Review the referenced JetBrains troubleshooting guidance.
- [x] Reproduce or confirm the local SSL failure conditions for the configured interpreter.
- [x] Explain the root cause and recommend the cleanest fix path.
- [x] Recreate `.venv` from a Python interpreter with working SSL support.
- [x] Reinstall project requirements into the rebuilt environment.
- [x] Repoint PyCharm to the rebuilt `.venv` and confirm package installation in the IDE.

## Risks

- A fix that depends on shell startup environment may work in terminals but still fail inside PyCharm package operations.
- Because Python 3.7 is old, some package installation issues may be confused with SSL issues even when the root cause is environment setup.

## Next Step

Keep PyCharm pointed at `D:\python\d2bot\.venv\Scripts\python.exe` as the canonical project interpreter.
