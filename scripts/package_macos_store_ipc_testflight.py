#!/usr/bin/env python3
"""Prepare a diagnostic Store candidate; does not upload or release it."""
import argparse
import datetime
import hashlib
import json
import os
import pathlib
import plistlib
import re
import shutil
import subprocess
import tomllib


def run(*args, **kwargs):
    return subprocess.run([str(arg) for arg in args], check=True, **kwargs)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('destination', 'gui', 'cli', 'probe', 'profile', 'app_identity',
                 'installer_identity', 'build'):
        parser.add_argument(name)
    parser.add_argument('--helper-profiles', type=pathlib.Path,
                        help='Optional directory containing Server/Client/WrongPeer/MissingGroup.provisionprofile')
    args = parser.parse_args()
    dest = pathlib.Path(args.destination)
    if not dest.is_absolute() or dest.exists():
        parser.error('Require a new absolute destination')
    if not re.fullmatch(r'[1-9][0-9]{0,17}(\.[0-9]{1,4}){0,2}', args.build):
        parser.error('Require an explicit numeric build number')
    gui, cli, probe, profile = (pathlib.Path(getattr(args, name)).resolve()
                                for name in ('gui', 'cli', 'probe', 'profile'))
    for path in (gui, cli, probe, profile):
        if not path.is_file():
            parser.error(f'Missing input: {path.name}')
    decoded = run('security', 'cms', '-D', '-i', profile, capture_output=True).stdout
    provisioning = plistlib.loads(decoded)
    entitlement = provisioning['Entitlements']
    team = provisioning['TeamIdentifier'][0]
    root_id = 'app.quickpresenter.QuickPresenter'
    if (entitlement.get('com.apple.application-identifier') != f'{team}.{root_id}'
            or entitlement.get('get-task-allow') or provisioning.get('ProvisionedDevices')
            or provisioning['ExpirationDate'] <= datetime.datetime.now(datetime.timezone.utc).replace(tzinfo=None)):
        parser.error('Require an unexpired product App Store distribution profile')
    version = tomllib.loads(pathlib.Path('Cargo.toml').read_text())['package']['version']
    dest.mkdir(parents=True)
    app = dest / 'Quick Presenter.app'
    run('scripts/stage_macos_app_bundle.sh', dest, gui, cli)
    # Production Store CLI is unsupported; only the independently sandboxed probes are included.
    (app / 'Contents/MacOS/qp').unlink()
    for info_path in [app / 'Contents/Info.plist', *(app / 'Contents').glob('**/Contents/Info.plist')]:
        info = plistlib.loads(info_path.read_bytes())
        info['LSMinimumSystemVersion'] = '14.4'
        info['CFBundleVersion'] = args.build
        if info_path == app / 'Contents/Info.plist':
            info['CFBundleDisplayName'] = 'Quick Presenter IPC Validation'
            info['NSHumanReadableCopyright'] = 'Copyright © 2026 Koichiro Ohba'
            info['LSApplicationCategoryType'] = 'public.app-category.productivity'
            sdk = run('xcrun', '--sdk', 'macosx', '--show-sdk-version', capture_output=True, text=True).stdout.strip()
            info['DTSDKName'] = f'macosx{sdk}'
            info['BuildMachineOSBuild'] = run('sw_vers', '-buildVersion', capture_output=True, text=True).stdout.strip()
        info_path.write_bytes(plistlib.dumps(info))
    shutil.copyfile(profile, app / 'Contents/embedded.provisionprofile')
    root_ent = plistlib.loads(pathlib.Path('packaging/macos/Store-UI.entitlements').read_bytes())
    root_ent['com.apple.application-identifier'] = f'{team}.{root_id}'
    root_ent['com.apple.developer.team-identifier'] = team
    root_ent_path = dest / 'UI.entitlements'
    root_ent_path.write_bytes(plistlib.dumps(root_ent))
    run('xattr', '-dr', 'com.apple.quarantine', app)
    # Sign the established renderer boundary before adding the diagnostic code.
    env = dict(os.environ, MACOS_DISTRIBUTION_MODE='app-store',
               MACOS_STORE_UI_ENTITLEMENTS=str(root_ent_path))
    run('scripts/sign_macos_app.sh', app, args.app_identity, env=env)
    probe_app = app / 'Contents/Helpers/StoreIPCProbe.app'
    entries = [('Server', 'server', probe_app)]
    entries += [(name, identifier, probe_app / f'Contents/Helpers/{name}.app')
                for name, identifier in [('Client', 'client'), ('WrongPeer', 'wrong'),
                                         ('MissingGroup', 'missing')]]
    signed = []
    for name, identifier, bundle in entries:
        contents = bundle / 'Contents'
        (contents / 'MacOS').mkdir(parents=True)
        shutil.copy2(probe, contents / 'MacOS/probe')
        bundle_id = f'app.quickpresenter.ipc-probe.{identifier}'
        info = dict(CFBundleExecutable='probe', CFBundleIdentifier=bundle_id,
                    CFBundleName=f'IPC Diagnostic {name}', CFBundlePackageType='APPL',
                    CFBundleVersion=args.build, CFBundleShortVersionString=version,
                    LSMinimumSystemVersion='14.4')
        (contents / 'Info.plist').write_bytes(plistlib.dumps(info))
        ent = {'com.apple.security.app-sandbox': True}
        if name != 'MissingGroup':
            ent['com.apple.security.application-groups'] = [f'{team}.app.quickpresenter.control-probe']
        if args.helper_profiles:
            helper_profile = args.helper_profiles / f'{name}.provisionprofile'
            helper_data = plistlib.loads(run('security', 'cms', '-D', '-i', helper_profile,
                                            capture_output=True).stdout)
            if (helper_data['Entitlements'].get('com.apple.application-identifier') != f'{team}.{bundle_id}'
                    or helper_data.get('ProvisionedDevices')
                    or helper_data['Entitlements'].get('get-task-allow')
                    or helper_data['ExpirationDate'] <= datetime.datetime.now(datetime.timezone.utc).replace(tzinfo=None)):
                parser.error(f'Invalid helper profile: {name}')
            shutil.copyfile(helper_profile, contents / 'embedded.provisionprofile')
            ent['com.apple.application-identifier'] = f'{team}.{bundle_id}'
            ent['com.apple.developer.team-identifier'] = team
        ent_path = dest / f'{name}.entitlements'
        ent_path.write_bytes(plistlib.dumps(ent))
        signed.append((bundle, ent_path))
    # Sign deepest helpers first, then the diagnostic server and original GUI.
    for bundle, ent_path in reversed(signed):
        run('codesign', '--force', '--timestamp', '--options', 'runtime', '--sign',
            args.app_identity, '--entitlements', ent_path, bundle)
    run('codesign', '--force', '--timestamp', '--options', 'runtime', '--sign',
        args.app_identity, '--entitlements', root_ent_path, app)
    run('python3', 'scripts/check_macos_store_private_apis.py', app)
    run('codesign', '--verify', '--deep', '--strict', app)
    package = dest / 'Quick-Presenter-IPC-TestFlight.pkg'
    run('productbuild', '--component', app, '/Applications', '--sign',
        args.installer_identity, package)
    run('pkgutil', '--check-signature', package)
    manifest = dict(purpose='Internal diagnostic validation only; not a customer release',
                    source=run('git', 'rev-parse', 'HEAD', capture_output=True, text=True).stdout.strip(),
                    version=version, build=args.build, helper_profiles_embedded=bool(args.helper_profiles),
                    inputs={name: hashlib.sha256(path.read_bytes()).hexdigest()
                            for name, path in [('gui', gui), ('probe', probe)]},
                    package_sha256=hashlib.sha256(package.read_bytes()).hexdigest())
    (dest / 'diagnostic-manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    print(json.dumps(manifest))
    print('Candidate only. Apple processing and installed-runtime gates remain open.')


if __name__ == '__main__':
    main()
