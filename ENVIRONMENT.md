# Environment Setup

This project uses a repo-local virtual environment.

## Why this exists

The system Anaconda installation can run Python, but SSL-dependent tools such as `pip` fail unless the Python DLL paths are added to `PATH` first.

To keep the fix local to this repo, use the activation script below instead of relying on a machine-wide Python setup.

## Activate the project environment

From PowerShell in the repo root:

```powershell
. .\scripts\activate-project.ps1
```

After that, `python` and `pip` should resolve to the local project environment.

## Verify

```powershell
python -c "import ssl; print(ssl.OPENSSL_VERSION)"
python -m pip --version
```

## Install dependencies

```powershell
python -m pip install -r requirements.txt
```
