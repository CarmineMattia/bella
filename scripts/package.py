#!/usr/bin/env python3
"""Create local alpha artifacts from the already-built release binary. No uploads."""
import argparse, hashlib, json, os, pathlib, shutil, subprocess, tarfile, tomllib
ROOT=pathlib.Path(__file__).resolve().parent.parent

def digest(path):
    with open(path,'rb') as stream:
        return hashlib.file_digest(stream,'sha256').hexdigest()

def main():
    parser=argparse.ArgumentParser();parser.add_argument('output',type=pathlib.Path,nargs='?',default=ROOT/'dist');args=parser.parse_args()
    version=tomllib.loads((ROOT/'Cargo.toml').read_text())['package']['version'];tag=f'term-social-{version}-linux-x86_64'
    binary=ROOT/'target/x86_64-unknown-linux-musl/release/term-social'
    actual=subprocess.check_output([binary,'--version'],text=True).strip()
    if actual!=f'term-social {version}':raise SystemExit(f'Release version mismatch: {actual}')
    output=args.output.expanduser().resolve();output.mkdir(parents=True,exist_ok=True)
    bundle=output/tag
    if bundle.exists():
        # Only replace this packager's own versioned runtime directory.
        marker=bundle/'MANIFEST.json'
        if not marker.exists() or json.loads(marker.read_text()).get('package')!=tag:raise SystemExit(f'Refusing to replace unrecognized directory: {bundle}')
        shutil.rmtree(bundle)
    (bundle/'bin').mkdir(parents=True);(bundle/'scripts').mkdir();(bundle/'tests').mkdir()
    shutil.copy2(binary,bundle/'bin/term-social');shutil.copy2(ROOT/'start',bundle/'start')
    for name in ('launch.py','start-model'):shutil.copy2(ROOT/'scripts'/name,bundle/'scripts'/name)
    for name in ('README.md','CHANGELOG.md','PLAN.md'):shutil.copy2(ROOT/name,bundle/name)
    shutil.copytree(ROOT/'server',bundle/'server',ignore=shutil.ignore_patterns('__pycache__'))
    shutil.copytree(ROOT/'docs',bundle/'docs',ignore=shutil.ignore_patterns('__pycache__'))
    for name in ('alpha_smoke.py','live_alpha.py','network_smoke.py'):shutil.copy2(ROOT/'tests'/name,bundle/'tests'/name)
    manifest={'package':tag,'version':version,'target':'x86_64-unknown-linux-musl','binary_sha256':digest(binary),'scope':'Linux alpha with local mode and opt-in authenticated hub sync','requires':['Python 3.11+','UTF-8 terminal','sh and GNU timeout for tools'],'model_weights_included':False,'user_data_included':False}
    (bundle/'MANIFEST.json').write_text(json.dumps(manifest,indent=2)+'\n')
    files=sorted(p for p in bundle.rglob('*') if p.is_file())
    (bundle/'SHA256SUMS').write_text(''.join(f'{digest(p)}  {p.relative_to(bundle).as_posix()}\n' for p in files))
    archive=output/f'{tag}.tar.gz'
    with tarfile.open(archive,'w:gz') as tar:tar.add(bundle,arcname=tag)
    source=output/f'term-social-{version}-source.tar.gz'
    names=['Cargo.toml','Cargo.lock','README.md','CHANGELOG.md','PLAN.md','.gitignore','run','start','src','scripts','tests','docs','server']
    def filter_source(info):
        if '__pycache__' in pathlib.PurePosixPath(info.name).parts or info.name.endswith('.pyc'):return None
        return info
    with tarfile.open(source,'w:gz') as tar:
        for name in names:tar.add(ROOT/name,arcname=f'term-social-{version}-source/{name}',filter=filter_source)
    (output/'SHA256SUMS').write_text(''.join(f'{digest(p)}  {p.name}\n' for p in (archive,source)))
    print(json.dumps({'runtime_directory':str(bundle),'runtime_archive':str(archive),'source_archive':str(source),'checksums':str(output/'SHA256SUMS')},indent=2))
if __name__=='__main__':main()
