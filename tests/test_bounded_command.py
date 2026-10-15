"""Exercise CI deadlines and inherited output handles with real processes."""
from pathlib import Path
import subprocess
import os
import signal
import base64
import sys
import tempfile
import time
import unittest

RUNNER = Path(__file__).resolve().parents[1] / 'scripts/run_bounded_command.py'


class BoundedCommandTests(unittest.TestCase):
    def invoke(self, directory, code, timeout=5):
        return subprocess.run(
            [sys.executable, str(RUNNER), '--timeout', str(timeout),
             '--label', 'regression probe', '--log-dir', str(directory),
             '--', sys.executable, '-c', code],
            capture_output=True, text=True, encoding='utf-8', timeout=20)

    def test_output_and_failure_are_preserved(self):
        with tempfile.TemporaryDirectory() as directory:
            result = self.invoke(directory, "import sys; print('JSON output'); "
                                 "print('diagnostic', file=sys.stderr); sys.exit(7)")
            self.assertEqual(result.returncode, 7)
            self.assertEqual(result.stdout.strip(), 'JSON output')
            self.assertIn('diagnostic', result.stderr)
            self.assertIn('regression probe', result.stderr)
            self.assertEqual(len(list(Path(directory).glob('*'))), 2)

    def test_parent_exit_does_not_wait_for_gui_inheriting_output(self):
        with tempfile.TemporaryDirectory() as directory:
            started = time.monotonic()
            result = self.invoke(directory, "import subprocess,sys; "
                                 "child = subprocess.Popen([sys.executable,'-c','import time; time.sleep(30)']); "
                                 "print(child.pid); print('parent finished')", timeout=10)
            elapsed = time.monotonic() - started
            pid = int(result.stdout.splitlines()[0])
            if os.name == 'nt':
                subprocess.run(['taskkill.exe', '/PID', str(pid), '/T', '/F'],
                               capture_output=True, timeout=10)
            else:
                os.kill(pid, signal.SIGKILL)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn('parent finished', result.stdout)
            self.assertLess(elapsed, 15)

    def test_timeout_terminates_descendant_before_it_writes(self):
        with tempfile.TemporaryDirectory() as directory:
            marker = Path(directory) / 'unexpected-child-result'
            child = f"import time,pathlib; time.sleep(2); pathlib.Path({str(marker)!r}).touch()"
            code = ("import subprocess,sys,time; "
                    f"subprocess.Popen([sys.executable,'-c',{child!r}]); time.sleep(30)")
            result = self.invoke(directory, code, timeout=0.5)
            self.assertEqual(result.returncode, 124, result.stderr)
            self.assertIn('Timed out: regression probe', result.stderr)
            time.sleep(2)
            self.assertFalse(marker.exists(), 'Timed-out descendant survived')

    @unittest.skipUnless(os.name == 'nt', 'Windows PowerShell integration')
    def test_powershell5_preserves_json_and_does_not_treat_diagnostics_as_errors(self):
        with tempfile.TemporaryDirectory() as directory:
            def literal(value):
                return "'" + str(value).replace("'", "''") + "'"

            script = RUNNER.with_name('check_windows_msix_sandbox.ps1')
            code = f"""
$ErrorActionPreference = 'Stop'
$runner = {literal(RUNNER)}
$logs = {literal(directory)}
Set-Alias python {literal(sys.executable)}
$errors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile(
    {literal(script)}, [ref]$null, [ref]$errors)
if ($errors.Count) {{ throw ($errors | Out-String) }}
$ast.FindAll({{ param($node)
    $node -is [System.Management.Automation.Language.FunctionDefinitionAst]
}}, $true) | ForEach-Object {{ . ([scriptblock]::Create($_.Extent.Text)) }}
$result = Invoke-BoundedCommand -Label 'PowerShell JSON probe' -Command @(
    {literal(sys.executable)}, '-c', 'import sys; print(''{{"success":true}}''); print(''diagnostic'', file=sys.stderr)')
if (-not ($result | ConvertFrom-Json).success) {{ throw 'Invalid JSON output' }}
Invoke-BoundedPowerShell -Label 'PowerShell cmdlet probe' -Code 'Write-Output ''cmdlet completed'''
"""
            encoded = base64.b64encode(code.encode('utf-16le')).decode('ascii')
            result = subprocess.run(['powershell.exe', '-NoProfile', '-NonInteractive',
                                     '-EncodedCommand', encoded], capture_output=True, timeout=60)
            self.assertEqual(result.returncode, 0, result.stderr.decode(errors='replace'))
            self.assertIn(b'cmdlet completed', result.stdout)


if __name__ == '__main__':
    unittest.main()
