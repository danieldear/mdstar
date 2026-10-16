"""Exercise signing setup without credentials, Keychain access, or Apple requests."""
import base64
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[2] / 'scripts/setup-macos-signing.sh'


class SigningSetupTests(unittest.TestCase):
    def run_setup(self, *, missing=None, certificate=None, identity=True, notary_ok=True):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            mock = root / 'bin'
            mock.mkdir()
            security = mock / 'security'
            security.write_text('''#!/bin/bash
printf '%s\\n' "$1" >> "$MOCK_LOG"
case "$1" in
  create-keychain) touch "${@: -1}" ;;
  list-keychains)
    if [ "$#" = 3 ]; then printf '    "/tmp/Existing Keychain.keychain-db"\\n'; fi ;;
  find-identity)
    if [ "$MOCK_IDENTITY" = yes ]; then
      echo '  1) AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA "Developer ID Application: Test (TESTTEAM)"'
    fi ;;
esac
''')
            security.chmod(0o700)
            xcrun = mock / 'xcrun'
            xcrun.write_text('#!/bin/bash\nexit "$MOCK_NOTARY_EXIT"\n')
            xcrun.chmod(0o700)
            env = dict(os.environ, PATH=f'{mock}:' + os.environ['PATH'],
                       CERTIFICATE_P12=certificate if certificate is not None else base64.b64encode(b'test fixture').decode(),
                       CERTIFICATE_PASSWORD='fixture', APPLE_ID='fixture@example.invalid',
                       APPLE_APP_PASSWORD='fixture', TEAM_ID='TESTTEAM',
                       RUNNER_TEMP=tmp, GITHUB_ENV=str(root / 'env'),
                       MOCK_LOG=str(root / 'commands'),
                       MOCK_IDENTITY='yes' if identity else 'no',
                       MOCK_NOTARY_EXIT='0' if notary_ok else '1')
            if missing:
                env.pop(missing, None)
            result = subprocess.run(['bash', str(SCRIPT)], env=env, capture_output=True, text=True)
            self.assertFalse((root / 'certificate.p12').exists(), 'temporary certificate must always be deleted')
            exported = (root / 'env').read_text() if (root / 'env').exists() else ''
            called = (root / 'commands').read_text() if (root / 'commands').exists() else ''
            return result, exported, called

    def test_each_missing_secret_fails_before_keychain_access(self):
        secrets = {
            'CERTIFICATE_P12': 'MACOS_CERTIFICATE_P12',
            'CERTIFICATE_PASSWORD': 'MACOS_CERTIFICATE_PASSWORD',
            'APPLE_ID': 'APPLE_ID',
            'APPLE_APP_PASSWORD': 'APPLE_APP_PASSWORD',
        }
        for name, secret in secrets.items():
            with self.subTest(name=name):
                result, _, called = self.run_setup(missing=name)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(f"repository Actions secret {secret}", result.stdout)
                self.assertEqual(called, '')

    def test_missing_workflow_and_runner_inputs_are_not_called_secrets(self):
        for name, source in [
            ('TEAM_ID', 'workflow env setting'),
            ('RUNNER_TEMP', 'GitHub Actions runner environment variable'),
            ('GITHUB_ENV', 'GitHub Actions runner environment variable'),
        ]:
            with self.subTest(name=name):
                result, _, called = self.run_setup(missing=name)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(f"{source} {name}", result.stdout)
                self.assertNotIn('secret', result.stdout)
                self.assertEqual(called, '')

    def test_invalid_base64_is_rejected(self):
        result, exported, called = self.run_setup(certificate='!not-base64!')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('not valid base64', result.stdout)
        self.assertIn('KEYCHAIN=', exported)
        self.assertEqual(called, '')

    def test_decoded_empty_certificate_is_rejected(self):
        result, _, called = self.run_setup(certificate='\n')
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(called, '')

    def test_missing_private_key_or_wrong_team_is_rejected(self):
        result, exported, _ = self.run_setup(identity=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('private key', result.stdout)
        self.assertIn('KEYCHAIN=', exported)
        self.assertNotIn('VOLT_SIGNING_IDENTITY=', exported)

    def test_notary_credentials_failure_still_registers_cleanup(self):
        result, exported, _ = self.run_setup(notary_ok=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('KEYCHAIN=', exported)

    def test_success_exports_explicit_identity(self):
        result, exported, _ = self.run_setup()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('VOLT_SIGNING_IDENTITY=' + 'A' * 40, exported)
        self.assertNotIn('fixture', exported)


if __name__ == '__main__':
    unittest.main()
