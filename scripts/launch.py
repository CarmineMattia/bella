#!/usr/bin/env python3
"""Term Social alpha launcher.

./start [app options]               Configure once, reuse/start a local model, open chat.
./start --chat-only [app options]   Open chat without starting a model.
./start --model-only               Start/reuse the configured local model, then exit.
./start --stop-model               Stop only a server started by this launcher.
./start --model-path FILE          Override the installed GGUF file for startup.
./start --server FILE              Override the llama-server executable.
./start --startup-timeout SECONDS  Model startup deadline (default 90 seconds).
./start --help                     Show launcher and app options.

Model servers started here keep running after chat exits so other local sessions can
use them. Use --stop-model when finished. Remote endpoints are reused, never started.
No model downloads or package installations are performed.
"""
from __future__ import annotations
import argparse, contextlib, fcntl, json, os, pathlib, shutil, signal, subprocess, sys, time
import urllib.error, urllib.parse, urllib.request

ROOT = pathlib.Path(__file__).resolve().parent.parent
DEFAULT_MODEL = 'Qwen3.6-35B-A3B-UD-Q4_K_XL'
DEFAULT_ENDPOINT = 'http://127.0.0.1:8080/v1'


def process(pid):
    try:
        tail = pathlib.Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()
        if tail[0] == 'Z':
            return None
        return {'identity': tail[19], 'exe': os.readlink(f'/proc/{pid}/exe')}
    except (OSError, ValueError, IndexError):
        return None


def owned(record):
    actual = process(record.get('pid', -1))
    try:
        return actual is not None and actual['identity'] == record.get('identity') and actual['exe'] == record.get('exe') and os.getpgid(record['pid']) == record['pid']
    except ProcessLookupError:
        return False


def write_record(path, value):
    temp = path.with_suffix('.tmp')
    with open(temp, 'w', encoding='utf-8') as stream:
        os.chmod(temp, 0o600)
        json.dump(value, stream)
        stream.flush()
        os.fsync(stream.fileno())
    os.replace(temp, path)


def read_record(path):
    if not path.exists():
        return None
    record = json.loads(path.read_text())
    if not isinstance(record, dict) or not isinstance(record.get('pid'), int):
        raise RuntimeError(f'Invalid model process record: {path}; no process was signalled')
    return record


def stop_owned(path):
    record = read_record(path)
    if record is None:
        return 'No launcher-owned model server to stop.'
    if owned(record):
        os.killpg(record['pid'], signal.SIGTERM)
        deadline = time.monotonic() + 5
        while owned(record) and time.monotonic() < deadline:
            time.sleep(.1)
        if owned(record):
            os.killpg(record['pid'], signal.SIGKILL)
    path.unlink(missing_ok=True)
    return 'Launcher-owned model server stopped. External servers were left running.'


def models(endpoint):
    try:
        # Model endpoints are explicit user configuration; ignore proxy settings for loopback.
        url = endpoint.rstrip('/') + '/models'
        host = urllib.parse.urlparse(url).hostname
        opener = urllib.request.build_opener(urllib.request.ProxyHandler({})) if host in ('127.0.0.1', 'localhost', '::1') else urllib.request.build_opener()
        with opener.open(url, timeout=1) as response:
            value = json.load(response)
        return True, [m['id'] for m in value.get('data', []) if isinstance(m, dict) and isinstance(m.get('id'), str)]
    except urllib.error.HTTPError:
        return True, []
    except (OSError, ValueError):
        return False, []


@contextlib.contextmanager
def model_lock(directory):
    directory.mkdir(parents=True, exist_ok=True)
    with open(directory / 'model.lock', 'a+', encoding='utf-8') as lock:
        os.chmod(lock.name, 0o600)
        fcntl.flock(lock, fcntl.LOCK_EX)
        yield


def ensure_model(endpoint, alias, directory, model_path, server, startup_timeout):
    endpoint = endpoint.rstrip('/')
    reached, available = models(endpoint)
    if alias in available:
        return 'Using the existing model server.'
    parsed = urllib.parse.urlparse(endpoint)
    if parsed.scheme != 'http' or parsed.hostname not in ('localhost', '127.0.0.1') or parsed.path.rstrip('/') != '/v1' or parsed.username or parsed.password or parsed.query or parsed.fragment:
        raise RuntimeError('Configured model is unavailable. Automatic startup supports http://127.0.0.1:PORT/v1 only; start other endpoints separately.')
    record_path = directory / 'model-process.json'
    with model_lock(directory):
        reached, available = models(endpoint)
        if alias in available:
            return 'Using the existing model server.'
        record = read_record(record_path)
        running = record is not None and owned(record)
        if running and record.get('endpoint') != endpoint:
            raise RuntimeError('This profile already manages a model on another endpoint. Stop it with --stop-model before changing the endpoint.')
        child = None
        if not running:
            if reached:
                raise RuntimeError('An existing server responds, but the selected model is unavailable. Inspect F2 or --doctor; no second server was started.')
            executable = shutil.which(server) if server else shutil.which('llama-server')
            if not executable:
                raise RuntimeError('llama-server is not installed. Set LLAMA_SERVER or use --server FILE. Chat can run without it.')
            model_path = pathlib.Path(model_path).expanduser().resolve()
            if not model_path.is_file():
                raise RuntimeError(f'Installed model not found: {model_path}. Use --model-path FILE or TERM_SOCIAL_MODEL_PATH.')
            log_path = directory / 'model.log'
            if log_path.exists() and log_path.stat().st_size > 10 * 1024 * 1024:
                os.replace(log_path, directory / 'model.previous.log')
            argv = [executable, '-m', str(model_path), '--alias', alias, '--host', '127.0.0.1', '--port', str(parsed.port or 80), '-c', os.environ.get('TERM_SOCIAL_MODEL_CONTEXT', '8192'), '-ngl', os.environ.get('TERM_SOCIAL_MODEL_GPU_LAYERS', '99'), '--flash-attn', 'on', '--jinja']
            with open(log_path, 'ab') as log:
                os.chmod(log_path, 0o600)
                child = subprocess.Popen(argv, stdin=subprocess.DEVNULL, stdout=log, stderr=log, start_new_session=True)
            actual = process(child.pid)
            if actual is None:
                child.wait(timeout=2)
                raise RuntimeError(f'Model server exited during startup. See {log_path}')
            record = {**actual, 'pid': child.pid, 'endpoint': endpoint, 'model_path': str(model_path), 'alias': alias}
            write_record(record_path, record)
        print(f'Preparing local model; log: {directory / "model.log"}', file=sys.stderr, flush=True)
        deadline = time.monotonic() + startup_timeout
        try:
            while time.monotonic() < deadline:
                if child is not None and child.poll() is not None:
                    raise RuntimeError(f'Model server exited with code {child.returncode}; see {directory / "model.log"}')
                if alias in models(endpoint)[1]:
                    return 'Local model ready. It stays available until --stop-model.'
                time.sleep(.4)
            raise RuntimeError(f'Model startup exceeded {startup_timeout}s; see {directory / "model.log"}')
        except BaseException:
            if child is not None:
                stop_owned(record_path)
                child.wait(timeout=6)
            raise


def main(argv=None):
    argv = list(sys.argv[1:] if argv is None else argv)
    parser = argparse.ArgumentParser(add_help=False, allow_abbrev=False)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument('--chat-only', action='store_true')
    mode.add_argument('--model-only', action='store_true')
    mode.add_argument('--stop-model', action='store_true')
    parser.add_argument('--model-path', default=os.environ.get('TERM_SOCIAL_MODEL_PATH', str(pathlib.Path.home() / 'models' / DEFAULT_MODEL / f'{DEFAULT_MODEL}.gguf')))
    parser.add_argument('--server', default=os.environ.get('LLAMA_SERVER'))
    parser.add_argument('--startup-timeout', type=float, default=90)
    launch, app_args = parser.parse_known_args(argv)
    if not 0 < launch.startup_timeout <= 3600:
        raise RuntimeError('--startup-timeout must be between 0 and 3600 seconds')
    options = argparse.ArgumentParser(add_help=False, allow_abbrev=False)
    for field in ('data', 'config', 'name', 'endpoint', 'model', 'workspace'):
        options.add_argument('--' + field)
    options.add_argument('--setup', action='store_true')
    profile_args, _ = options.parse_known_args(app_args)
    binary = next((p for p in (ROOT/'bin/term-social', ROOT/'target/x86_64-unknown-linux-musl/release/term-social', ROOT/'target/x86_64-unknown-linux-musl/debug/term-social') if p.is_file()), None)
    if binary is None:
        raise RuntimeError('App binary not found. From source, run ./scripts/build-alpha. Use the extracted alpha package to run without Rust.')
    if '--help' in app_args or '-h' in app_args:
        print(__doc__, flush=True)
        return subprocess.call([str(binary), '--help'])
    xdg = os.environ.get('XDG_DATA_HOME')
    user_data = pathlib.Path(xdg) if xdg and pathlib.Path(xdg).is_absolute() else pathlib.Path.home()/'.local/share'
    data = pathlib.Path(profile_args.data or os.environ.get('TERM_SOCIAL_DATA') or (str(ROOT/'.data/chat.sqlite') if (ROOT/'Cargo.toml').exists() else str(user_data/'term-social/chat.sqlite'))).expanduser().resolve()
    config = pathlib.Path(profile_args.config or os.environ.get('TERM_SOCIAL_CONFIG') or data.parent/'settings.json').expanduser().resolve()
    env = {**os.environ, 'TERM_SOCIAL_DATA':str(data), 'TERM_SOCIAL_CONFIG':str(config)}
    headless = any(flag in app_args for flag in ('--version','-V','--send','--history','--doctor','--backup','--setup-only','--network-logout'))
    if '--version' not in app_args and '-V' not in app_args:
        data.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    if launch.stop_model:
        with model_lock(config.parent):
            print(stop_owned(config.parent/'model-process.json'))
        return 0
    if not headless and not launch.model_only and (not sys.stdin.isatty() or not sys.stdout.isatty()):
        raise RuntimeError('Chat needs an interactive terminal. Headless options include --model-only, --send, --history, --doctor and --backup.')
    if not config.exists() and not profile_args.workspace and not env.get('TERM_SOCIAL_WORKSPACE') and not headless:
        workspace = data.parent/'workspace'
        workspace.mkdir(parents=True, exist_ok=True)
        env['TERM_SOCIAL_WORKSPACE'] = str(workspace)
    if not headless and not launch.model_only and (profile_args.setup or (not config.exists() and not profile_args.name)):
        setup_args = [v for v in app_args if v != '--setup']
        profile_setup_args = [v for v in setup_args if v not in ('--network', '--network-setup')]
        result = subprocess.call([str(binary), '--setup-only', *profile_setup_args], env=env)
        if result != 0:
            return result
        app_args = setup_args
    settings = json.loads(config.read_text()) if config.exists() and not headless else {}
    if config.exists() and not headless and (not isinstance(settings, dict) or set(settings) != {'name','endpoint','model','workspace'} or not all(isinstance(v,str) for v in settings.values())):
        raise RuntimeError('Invalid saved profile. Run ./start --setup to repair it; no model was started.')
    endpoint = profile_args.endpoint or settings.get('endpoint', DEFAULT_ENDPOINT)
    alias = profile_args.model or settings.get('model', DEFAULT_MODEL)
    if not launch.chat_only and (launch.model_only or not headless):
        try:
            print(ensure_model(endpoint, alias, config.parent, launch.model_path, launch.server, launch.startup_timeout), file=sys.stderr)
        except (OSError, RuntimeError, ValueError) as error:
            print(f'Model unavailable: {error}', file=sys.stderr)
            if launch.model_only:
                return 1
            print('Opening chat. F2 shows model status; --chat-only skips model startup.', file=sys.stderr)
    if launch.model_only:
        return 0
    # exec preserves signals and the app exit status; the model has its own process group.
    os.execve(binary, [str(binary), *app_args], env)


if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except KeyboardInterrupt:
        print('\nStartup cancelled.', file=sys.stderr)
        raise SystemExit(130)
    except (OSError, RuntimeError, ValueError) as error:
        print(f'Term Social launcher: {error}', file=sys.stderr)
        raise SystemExit(1)
