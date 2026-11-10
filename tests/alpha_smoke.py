#!/usr/bin/env python3
"""Run the alpha's real terminal and launcher flows against isolated temporary data.
Only standard-library Python is required. Fake HTTP model servers make this repeatable
without loading weights. The real installed model is checked separately by the Rust live test.
"""
import argparse, contextlib, fcntl, http.server, json, os, pathlib, pty, re, shutil, signal, socket, sqlite3, struct, subprocess, sys, tempfile, termios, threading, time, unittest

OPTIONS = None


def eventually(check, seconds=6):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        value = check()
        if value:
            return value
        time.sleep(.05)
    raise AssertionError('Expected state did not arrive before deadline')


class Terminal:
    def __init__(self, command, cwd, env=None):
        self.master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 36, 120, 0, 0))
        self.chunks = []
        self.process = subprocess.Popen(command, stdin=slave, stdout=slave, stderr=slave, cwd=cwd, env={**os.environ, 'TERM':'xterm-256color', **(env or {})})
        os.close(slave)
        def drain():
            try:
                while True:
                    block = os.read(self.master, 65536)
                    if not block:
                        return
                    self.chunks.append(block)
            except OSError:
                pass
        self.reader = threading.Thread(target=drain, daemon=True)
        self.reader.start()

    def send(self, value):
        os.write(self.master, value.encode())
        time.sleep(.08)

    def click(self, x, y, button=0):
        self.send(f'\x1b[<{button};{x};{y}M\x1b[<{button};{x};{y}m')

    def screen(self):
        value = b''.join(self.chunks).decode('utf-8', 'replace')
        grid = [[' ']*120 for _ in range(36)]
        x = y = i = 0
        while i < len(value):
            if value[i] == '\x1b':
                if value.startswith('\x1b]', i):
                    endings = [(value.find('\x07', i + 2), 1), (value.find('\x1b\\', i + 2), 2)]
                    endings = [(end, size) for end, size in endings if end >= 0]
                    if not endings: break
                    end, size = min(endings)
                    i = end + size; continue
                match = re.match(r'\x1b\[([0-9;?]*)([ -/]*)([@-~])', value[i:])
                if match:
                    params, _, action = match.groups()
                    nums = [int(v or 1) for v in params.split(';')] if not params.startswith('?') else []
                    if action in ('H','f') and nums:
                        y = max(0, min(35, nums[0]-1)); x = max(0, min(119, (nums[1] if len(nums)>1 else 1)-1))
                    elif action == 'J' and nums and nums[0] == 2:
                        grid = [[' ']*120 for _ in range(36)]
                    elif action == 'K':
                        grid[y][x:] = [' ']*(120-x)
                    elif action == 'C': x = min(119, x+(nums[0] if nums else 1))
                    elif action == 'D': x = max(0, x-(nums[0] if nums else 1))
                    elif action == 'A': y = max(0, y-(nums[0] if nums else 1))
                    elif action == 'B': y = min(35, y+(nums[0] if nums else 1))
                    i += len(match[0]); continue
                i += 2; continue
            character = value[i]; i += 1
            if character == '\r': x = 0
            elif character == '\n': y = min(35,y+1)
            elif ord(character) >= 32:
                grid[y][x] = character; x = min(119,x+1)
        return '\n'.join(''.join(line) for line in grid)

    def wait_text(self, text):
        try:
            return eventually(lambda: text in self.screen())
        except AssertionError as error:
            raise AssertionError(f'{text!r} absent from terminal:\n{self.screen()}') from error

    def close(self, crash=False):
        if self.process.poll() is None:
            if crash:
                self.process.kill()
            else:
                self.send('\x03')
            try:
                self.process.wait(timeout=6)
            except subprocess.TimeoutExpired:
                self.process.kill(); self.process.wait(timeout=3)
                raise
        with contextlib.suppress(OSError): os.close(self.master)
        if not crash:
            assert self.process.returncode == 0, f'Terminal exited {self.process.returncode}'


class Model:
    def __init__(self, mode='stream', command=None):
        self.mode = mode; self.command = command; self.bodies = []; self.requests = 0; self.checks = 0; self.disconnected = threading.Event()
        parent = self
        class Handler(http.server.BaseHTTPRequestHandler):
            protocol_version = 'HTTP/1.1'
            def log_message(self, *args): pass
            def do_GET(self):
                parent.checks += 1
                payload = b'{"data":[{"id":"fixture"}]}'
                self.send_response(200); self.send_header('Content-Type','application/json'); self.send_header('Content-Length',str(len(payload))); self.end_headers(); self.wfile.write(payload)
            def do_POST(self):
                parent.bodies.append(json.loads(self.rfile.read(int(self.headers['Content-Length'])))); parent.requests += 1
                if parent.mode == 'tool':
                    message = {'role':'assistant','content':None,'tool_calls':[{'id':'one','type':'function','function':{'name':'shell','arguments':json.dumps({'command':parent.command})}}]}
                    payload = json.dumps({'choices':[{'message':message}]}).encode()
                    self.send_response(200); self.send_header('Content-Type','application/json'); self.send_header('Content-Length',str(len(payload))); self.end_headers(); self.wfile.write(payload); return
                self.send_response(200); self.send_header('Content-Type','text/event-stream'); self.send_header('Connection','close'); self.end_headers(); self.close_connection = True
                text = 'partial text received' if parent.requests == 1 and parent.mode != 'complete' else 'second request completed'
                self.wfile.write(('data: '+json.dumps({'choices':[{'delta':{'content':text}}]})+'\n\n').encode()); self.wfile.flush()
                if parent.requests == 1 and parent.mode != 'complete':
                    self.connection.settimeout(6)
                    try:
                        if self.connection.recv(1) == b'': parent.disconnected.set()
                    except (ConnectionError, TimeoutError, OSError): pass
                else:
                    self.wfile.write(b'data: {"choices":[{"delta":{},"finish_reason":"stop"}]}\n\ndata: [DONE]\n\n'); self.wfile.flush()
        self.server = http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler)
        self.thread = threading.Thread(target=self.server.serve_forever,daemon=True); self.thread.start()
        self.endpoint = f'http://127.0.0.1:{self.server.server_port}/v1'
    def close(self): self.server.shutdown(); self.server.server_close()


class Alpha(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='term-social-alpha-')
        self.root = pathlib.Path(self.temporary.name); self.data = self.root/'chat.sqlite'; self.terminals = []; self.models = []
    def tearDown(self):
        for terminal in self.terminals:
            if terminal.process.poll() is None:
                with contextlib.suppress(Exception): terminal.close(crash=True)
        for model in self.models: model.close()
        self.temporary.cleanup()
    def sql(self, sql, params=()):
        with contextlib.closing(sqlite3.connect(self.data)) as db, db:
            return db.execute(sql,params).fetchall()
    def app(self, name='alice', room='lobby', model=None, launcher=False, first=False, env=None):
        command = [str(OPTIONS.launcher), '--chat-only'] if launcher else [str(OPTIONS.binary)]
        command += ['--data',str(self.data),'--config',str(self.root/'settings.json')]
        if not first: command += ['--name',name]
        if room != 'lobby': command += ['--room',room]
        if model: command += ['--endpoint',model.endpoint,'--model','fixture']
        terminal = Terminal(command,self.root,env=env); self.terminals.append(terminal)
        if not first: terminal.wait_text('F5 Activity')
        return terminal
    def post(self, room, body, name='bob'):
        subprocess.run([str(OPTIONS.binary),'--data',str(self.data),'--name',name,'--room',room,'--send',body],cwd=self.root,check=True,capture_output=True)
    def model(self,*args):
        model = Model(*args); self.models.append(model); return model

    def test_copy_message_click_preserves_exact_text_with_a_native_helper_fixture(self):
        helpers = self.root/'helpers'; helpers.mkdir()
        helper = helpers/'wl-copy'
        helper.write_text('#!/bin/sh\ncat > "$TERM_SOCIAL_TEST_CLIPBOARD"\n'); helper.chmod(0o700)
        copied = self.root/'clipboard-text'
        terminal = self.app(model=self.model('complete'), env={
            'PATH':str(helpers)+os.pathsep+os.environ.get('PATH',''),
            'WAYLAND_DISPLAY':'fixture', 'TERM_SOCIAL_TEST_CLIPBOARD':str(copied)})
        texts = ["hello 界\nsecond line", "```sh\nprintf '%s' \"$HOME\"\n```", "Exit: 1\nsudo: a password is required"]
        for (author, kind), text in zip([('alice','human'),("alice's bot",'bot'),('tools','tool')], texts):
            self.sql("INSERT INTO messages(room,author,body,kind) VALUES('lobby',?,?,?)",(author,text,kind))
        eventually(lambda: terminal.screen().count('[ Copy ]') == 3)
        buttons = [(i,line.index('[ Copy ]')) for i,line in enumerate(terminal.screen().splitlines()) if '[ Copy ]' in line]
        for (y,x), text in zip(buttons,texts):
            terminal.click(x+1,y+1)
            eventually(lambda: copied.exists() and copied.read_text() == text)
            terminal.wait_text('Message copied')
            eventually(lambda: '[Copied]' in terminal.screen().splitlines()[y])
        terminal.close()

    def test_copy_terminal_fallback_is_encoded_and_does_not_claim_native_success(self):
        import base64
        terminal = self.app(model=self.model('complete'),env={'DISPLAY':'','WAYLAND_DISPLAY':''})
        text = 'hello 界\nexact second line'
        self.sql("INSERT INTO messages(room,author,body,kind) VALUES('lobby','alice',?,'human')",(text,))
        terminal.wait_text('[ Copy ]')
        rows = terminal.screen().splitlines(); y = next(i for i,line in enumerate(rows) if '[ Copy ]' in line); x = rows[y].index('[ Copy ]')
        terminal.click(x+1,y+1)
        expected = b'\x1b]52;c;'+base64.b64encode(text.encode())+b'\x1b\\'
        eventually(lambda: expected in b''.join(terminal.chunks))
        terminal.wait_text('Copy sent to terminal')
        self.assertNotIn('Message copied',terminal.screen())
        terminal.send('still responsive\r')
        eventually(lambda:self.sql("SELECT 1 FROM messages WHERE body='still responsive'"))
        terminal.close()

    def test_browser_details_click_opens_the_selected_action_without_executing_tools(self):
        model = self.model('complete'); terminal = self.app(model=model)
        for target in ['FIRST', 'SECOND']:
            record = 'Browser action v1: ' + json.dumps({
                'state':'launch_requested', 'target':target,
                'command':'fake-launch-only', 'diagnostics':f'PRIVATE_DIAGNOSTIC_{target}'})
            self.sql("INSERT INTO messages(room,author,body,kind) VALUES('lobby','tools',?,'tool')", (record,))
        eventually(lambda: terminal.screen().count('[ Details ]') == 2)
        rows = terminal.screen().splitlines()
        y = next(i for i, line in enumerate(rows) if '[ Details ]' in line)
        x = rows[y].index('[ Details ]')
        terminal.click(x + 1, y + 1)
        terminal.wait_text('PRIVATE_DIAGNOSTIC_FIRST')
        self.assertNotIn('PRIVATE_DIAGNOSTIC_SECOND', terminal.screen())
        terminal.send('\x1b[C'); terminal.wait_text('PRIVATE_DIAGNOSTIC_SECOND')
        terminal.send('\x1b'); terminal.send('/browser\r')
        terminal.wait_text('PRIVATE_DIAGNOSTIC_SECOND')
        terminal.send('\x1b'); terminal.send('/join another-room\r')
        eventually(lambda: '[ Details ]' not in terminal.screen())
        self.assertEqual(model.requests, 0)
        terminal.close()

    def test_profile_pictures_and_named_bots_persist_and_route(self):
        import zlib
        def chunk(kind, data):
            return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data))
        picture=self.root/'my avatar.png'
        picture.write_bytes(b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',8,8,8,2,0,0,0))+chunk(b'IDAT',zlib.compress((b'\x00'+bytes([55,170,220])*8)*8))+chunk(b'IEND',b''))
        model=self.model('complete'); terminal=self.app(model=model)
        terminal.send('/profile\r');terminal.wait_text('Your profile')
        terminal.send('\x15Alice Builder\tHere to build\t')
        terminal.send('\x1b[200~'+str(picture)+'\x1b[201~');terminal.send('\x13')
        eventually(lambda:self.sql("SELECT data FROM profiles WHERE owner='alice' AND handle=''") and json.loads(self.sql("SELECT data FROM profiles WHERE owner='alice' AND handle=''")[0][0])['name']=='Alice Builder')
        profile=json.loads(self.sql("SELECT data FROM profiles WHERE owner='alice' AND handle=''")[0][0]);self.assertEqual(len(profile['avatar']),576)
        terminal.send('/bots\r');terminal.wait_text('Bots ·')
        terminal.send('n');terminal.wait_text('Create a bot')
        terminal.send('planner\t\x15Planner\tPlanning helper\tUse numbered plans.\tfixture\t')
        terminal.send('\x1b[200~'+str(picture)+'\x1b[201~');terminal.send('\x13')
        terminal.wait_text('@planner');terminal.send('\x1b')
        terminal.send('@planner make a plan\r')
        eventually(lambda:self.sql("SELECT body FROM messages WHERE author=? AND kind='bot'",("alice's @planner",)))
        self.assertEqual(model.bodies[-1]['model'],'fixture')
        self.assertTrue(any('Use numbered plans.' in m.get('content','') for m in model.bodies[-1]['messages'] if m['role']=='system'))
        self.assertEqual(self.sql("SELECT author FROM messages WHERE kind='human' ORDER BY id DESC LIMIT 1")[0][0],'alice')
        second=self.app(name='bob',model=model);second.send('/people\r');second.wait_text('Alice Builder')
        # The imported thumbnail remains usable after its original file is removed.
        picture.unlink();terminal.close();terminal=self.app(model=model)
        terminal.send('/profile\r');terminal.wait_text('Alice Builder');terminal.send('\x1b')
        terminal.send('/bots\r');terminal.wait_text('@planner')
        terminal.send('\x1b[B\r');terminal.wait_text('Not in this chat')
        terminal.send('\x1b');terminal.send('@planner disabled\r');terminal.wait_text('Bot is not enabled here')
        self.assertEqual(len(model.bodies),1)
        terminal.close();second.close()

    def test_recipient_switch_routes_followups_and_preserves_per_chat_choice(self):
        model=self.model('complete');terminal=self.app(model=model)
        terminal.wait_text('To group')
        # Click Bot above the composer; the first untagged message must invoke it.
        terminal.click(55,32);terminal.wait_text('To @bot')
        terminal.send('first question without a tag\r')
        eventually(lambda:len(model.bodies)==1)
        eventually(lambda:self.sql("SELECT 1 FROM agent_tasks WHERE state='completed'"))
        self.assertEqual(self.sql("SELECT body FROM messages WHERE kind='human' ORDER BY id DESC LIMIT 1")[0][0],'@bot first question without a tag')
        terminal.send('follow up without a tag\r');eventually(lambda:len(model.bodies)==2)
        eventually(lambda:self.sql("SELECT COUNT(*) FROM agent_tasks WHERE state='completed'")[0][0]==2)
        terminal.send('draft keep');terminal.click(40,32);terminal.wait_text('To group');terminal.send('!\r')
        eventually(lambda:self.sql("SELECT 1 FROM messages WHERE body='draft keep!'"))
        self.assertEqual(len(model.bodies),2)
        terminal.send('\x1b[21~');terminal.wait_text('To @bot') # F10
        terminal.close();terminal=self.app(model=model);terminal.wait_text('To @bot')
        terminal.send('/join workshop\r');terminal.wait_text('To group')
        terminal.send('/join lobby\r');terminal.wait_text('To @bot')
        terminal.send('/to group\r');terminal.wait_text('To group')
        # A normal @mention selects the bot for subsequent untagged follow-ups.
        terminal.send('@bot explicit mention\r');eventually(lambda:len(model.bodies)==3)
        eventually(lambda:self.sql("SELECT COUNT(*) FROM agent_tasks WHERE state='completed'")[0][0]==3)
        terminal.wait_text('To @bot')
        terminal.send('/bots\r');terminal.wait_text('Bots ·');terminal.send('n');terminal.wait_text('Create a bot')
        terminal.send('planner\t\x15Planner\tPlanning helper\tPlan carefully\tfixture\t\x13');terminal.wait_text('@planner');terminal.send('\x1b')
        terminal.send('\x1b[21~');terminal.wait_text('To @planner')
        terminal.send('named bot without a tag\r');eventually(lambda:len(model.bodies)==4)
        eventually(lambda:self.sql("SELECT 1 FROM messages WHERE kind='bot' AND author=?",("alice's @planner",)))
        self.assertEqual(self.sql("SELECT body FROM messages WHERE kind='human' ORDER BY id DESC LIMIT 1")[0][0],'@planner named bot without a tag')
        self.assertTrue(any('Plan carefully' in message.get('content','') for message in model.bodies[-1]['messages'] if message['role']=='system'))
        terminal.close()

    def test_version_help_and_noninteractive_error_without_rust(self):
        env={**os.environ,'PATH':'/usr/bin:/bin','CARGO_HOME':'/nonexistent','RUSTUP_HOME':'/nonexistent'}
        version=subprocess.run([str(OPTIONS.launcher),'--version'],cwd=self.root,env=env,check=True,capture_output=True,text=True)
        self.assertEqual(version.stdout.strip(),'term-social 0.1.0-alpha.3')
        help_text=subprocess.run([str(OPTIONS.launcher),'--help'],cwd=self.root,env=env,check=True,capture_output=True,text=True)
        self.assertIn('--chat-only',help_text.stdout);self.assertIn('--backup',help_text.stdout)
        no_tty=subprocess.run([str(OPTIONS.launcher),'--chat-only','--name','alice','--data',str(self.data)],cwd=self.root,env=env,capture_output=True,text=True)
        self.assertNotEqual(no_tty.returncode,0);self.assertIn('interactive terminal',no_tty.stderr)

    def test_portable_default_data_directory_and_empty_xdg_fallback(self):
        portable=self.root/'portable app';(portable/'bin').mkdir(parents=True);(portable/'scripts').mkdir()
        shutil.copy2(OPTIONS.launcher,portable/'start')
        shutil.copy2(OPTIONS.launcher.parent/'scripts/launch.py',portable/'scripts/launch.py')
        (portable/'bin/term-social').symlink_to(OPTIONS.binary)
        for xdg,home,expected in [(str(self.root/'xdg'),self.root/'home-one',self.root/'xdg/term-social'),('',self.root/'home-two',self.root/'home-two/.local/share/term-social')]:
            terminal=Terminal([str(portable/'start'),'--chat-only'],self.root,env={'HOME':str(home),'XDG_DATA_HOME':xdg,'TERM_SOCIAL_DATA':'','TERM_SOCIAL_CONFIG':'','TERM_SOCIAL_WORKSPACE':''})
            self.terminals.append(terminal);terminal.wait_text('Welcome to Term Social');terminal.send('\x15portable-user\r');terminal.wait_text('F5 Activity')
            terminal.send('portable default data\r')
            eventually(lambda:(expected/'chat.sqlite').exists() and (expected/'settings.json').exists())
            terminal.close()
            with contextlib.closing(sqlite3.connect(expected/'chat.sqlite')) as db:
                self.assertEqual(db.execute("SELECT body FROM messages WHERE author='portable-user'").fetchone()[0],'portable default data')
            self.assertFalse((portable/'.data').exists())

    def test_welcome_mouse_paste_and_return_button_keep_the_saved_draft(self):
        terminal = self.app(launcher=True,first=True)
        terminal.wait_text('Welcome to Term Social')
        terminal.send('\x15\x1b[200~welcome-user\x1b[201~')
        rows = terminal.screen().splitlines()
        y = next(i for i,line in enumerate(rows) if 'Open chat' in line and 'Tab Next' not in line)
        x = rows[y].index('Open chat')
        terminal.click(x+1,y+1)
        terminal.wait_text('F5 Activity')
        terminal.send('draft survives welcome')
        eventually(lambda:self.sql("SELECT 1 FROM chat_state WHERE draft='draft survives welcome'"))
        rows = terminal.screen().splitlines(); y = next(i for i,line in enumerate(rows) if 'Exit to welcome' in line); x = rows[y].index('Exit to welcome')
        terminal.click(x+1,y+1)
        terminal.wait_text('Welcome to Term Social')
        terminal.send('\r')
        terminal.wait_text('F5 Activity'); terminal.wait_text('draft survives welcome')
        self.assertEqual(json.loads((self.root/'settings.json').read_text())['name'],'welcome-user')
        terminal.send('\x15/welcome\r'); terminal.wait_text('Welcome to Term Social')
        saved = (self.root/'settings.json').read_bytes()
        terminal.send('\x15do-not-save'); terminal.send('\x1b'); terminal.process.wait(timeout=4)
        self.assertEqual((self.root/'settings.json').read_bytes(),saved)

    def test_return_to_welcome_stops_an_active_response_before_reopening_setup(self):
        model = self.model(); terminal = self.app(model=model)
        terminal.send('/agent\r'); terminal.send('start a reply\r')
        eventually(lambda:self.sql("SELECT 1 FROM agent_tasks WHERE state='thinking'"))
        terminal.wait_text('partial text received')
        rows = terminal.screen().splitlines(); y = next(i for i,line in enumerate(rows) if 'Exit to welcome' in line); x = rows[y].index('Exit to welcome')
        terminal.click(x+1,y+1)
        terminal.wait_text('Welcome to Term Social')
        self.assertTrue(model.disconnected.wait(3))
        self.assertEqual(self.sql("SELECT state FROM agent_tasks ORDER BY id DESC LIMIT 1")[0][0],'cancelled')
        self.assertFalse(self.sql("SELECT 1 FROM local_presence"))
        terminal.send('\x1b'); terminal.process.wait(timeout=4)

    def test_return_to_welcome_waits_for_an_already_approved_command(self):
        completed = self.root/'completed-command'
        command = f"sleep .4; printf finished > '{completed}'; printf command-finished"
        terminal = self.app(model=self.model('tool',command))
        terminal.send('/agent\r');terminal.send('run the fixture command\r')
        terminal.wait_text('Review a shell command');terminal.send('y')
        rows = terminal.screen().splitlines(); y = next(i for i,line in enumerate(rows) if 'Exit to welcome' in line); x = rows[y].index('Exit to welcome')
        terminal.click(x+1,y+1)
        terminal.wait_text('Welcome to Term Social')
        self.assertEqual(completed.read_text(),'finished')
        self.assertTrue(self.sql("SELECT 1 FROM messages WHERE kind='tool' AND body LIKE '%command-finished%'"))
        self.assertEqual(self.sql("SELECT state FROM agent_tasks ORDER BY id DESC LIMIT 1")[0][0],'cancelled')
        self.assertFalse(self.sql("SELECT 1 FROM local_presence"))
        terminal.send('\x1b');terminal.process.wait(timeout=4)

    def test_first_run_saves_settings_and_reuses_them(self):
        terminal = self.app(launcher=True,first=True)
        terminal.wait_text('Welcome to Term Social')
        terminal.send('\x15first-user\r')
        eventually(lambda:(self.root/'settings.json').exists())
        eventually(lambda:self.data.exists() and self.sql("SELECT 1 FROM local_presence WHERE owner='first-user'"))
        terminal.wait_text('F5 Activity')
        terminal.send('saved profile message\r')
        eventually(lambda:self.sql("SELECT 1 FROM messages WHERE author='first-user'"))
        terminal.close()
        saved=(self.root/'settings.json').read_bytes()
        cancelled=Terminal([str(OPTIONS.launcher),'--chat-only','--setup','--data',str(self.data),'--config',str(self.root/'settings.json')],self.root)
        self.terminals.append(cancelled);cancelled.wait_text('Welcome to Term Social');cancelled.send('\x15discard-this-name');cancelled.send('\x1b');cancelled.process.wait(timeout=4);cancelled.close(crash=True)
        self.assertEqual(cancelled.process.returncode,130);self.assertEqual((self.root/'settings.json').read_bytes(),saved)
        reopened = self.app(launcher=True,first=True)
        reopened.wait_text('saved profile message'); reopened.close()

    def test_chat_mouse_unicode_drafts_and_people(self):
        alice = self.app(); alice.wait_text('Write a message')
        alice.click(3,4); alice.send('studio\r')
        eventually(lambda:self.sql("SELECT 1 FROM conversations WHERE room='studio'"))
        bob = self.app('bob','studio'); bob.send('hello alice\r')
        eventually(lambda:self.sql("SELECT 1 FROM messages WHERE body='hello alice'"))
        alice.send('unfinished draft'); alice.send('\x0b'); alice.send('my agent\r'); alice.send('\x0b'); alice.send('studio\r')
        eventually(lambda:self.sql("SELECT draft FROM chat_state WHERE owner='alice' AND room='studio'")==[('unfinished draft',)])
        alice.send('\x15'); alice.send('\x1b[200~ciao 界\nsecond line\x1b[201~'); alice.click(115,33)
        eventually(lambda:self.sql("SELECT 1 FROM messages WHERE body=?",('ciao 界\nsecond line',)))
        alice.send('/people\r'); alice.wait_text('People in this conversation'); alice.wait_text('bob')
        bob.close(); alice.wait_text('not viewing this room'); alice.send('\x1b')
        alice.send('draft recovered'); alice.close()
        reopened = self.app('alice','studio'); reopened.send('!\r')
        eventually(lambda:self.sql("SELECT 1 FROM messages WHERE body='draft recovered!'")); reopened.close()

    def test_history_search_and_unread_protection(self):
        self.post('lobby','ancient marker found')
        with contextlib.closing(sqlite3.connect(self.data)) as db, db:
            db.executemany("INSERT INTO messages(room,author,body,kind) VALUES('lobby','bob',?,'human')",[(f'newer message {i}',) for i in range(449)])
        terminal = self.app(); terminal.wait_text('newer message 448')
        terminal.click(115,2); terminal.send('ancient marker'); terminal.wait_text('ancient marker found')
        terminal.click(33,13); terminal.wait_text('Search result highlighted')
        self.post('lobby','incoming while reading old history')
        time.sleep(.2)
        self.assertEqual(self.sql("SELECT last_read FROM chat_state WHERE owner='alice' AND room='lobby'")[0][0],450)
        terminal.click(65,31); terminal.wait_text('incoming while reading old history')
        eventually(lambda:self.sql("SELECT last_read FROM chat_state WHERE owner='alice' AND room='lobby'")==[(451,)])
        terminal.close()

    def test_pin_archive_restore_and_scrolled_context_menu(self):
        terminal = self.app(room='studio'); terminal.send('hello\r'); self.post('project','newer chat'); terminal.send('draft keep')
        terminal.send('\x1bOR'); terminal.send('p')
        eventually(lambda:self.sql("SELECT pinned FROM conversation_preferences WHERE owner='alice' AND room='studio'")==[(1,)])
        terminal.click(89,2); terminal.click(34,19)
        eventually(lambda:self.sql("SELECT archived FROM conversation_preferences WHERE owner='alice' AND room='studio'")==[(1,)])
        terminal.send('\x0b'); terminal.send('project\r'); self.post('studio','incoming in archive')
        time.sleep(.2); read=self.sql("SELECT last_read FROM chat_state WHERE owner='alice' AND room='studio'")[0][0]
        self.assertLess(read,self.sql("SELECT MAX(id) FROM messages WHERE room='studio'")[0][0])
        terminal.click(25,1); terminal.click(6,6,2); terminal.send('a')
        eventually(lambda:self.sql("SELECT archived FROM conversation_preferences WHERE owner='alice' AND room='studio'")==[(0,)])
        self.assertEqual(self.sql("SELECT last_read FROM chat_state WHERE owner='alice' AND room='studio'")[0][0],read)
        self.assertFalse(self.sql("SELECT 1 FROM conversation_preferences WHERE owner='bob'"))
        terminal.close(); terminal=self.app(room='studio'); terminal.send('!\r')
        eventually(lambda:self.sql("SELECT 1 FROM messages WHERE body='draft keep!'"))
        for i in range(15):self.post(f'dummy-{i:02}','scroll fixture')
        time.sleep(.2); terminal.send('\x1b[<65;3;15M'); terminal.send('\x1b[<65;3;15M'); terminal.click(6,6,2); terminal.send('p')
        eventually(lambda:self.sql("SELECT pinned FROM conversation_preferences WHERE owner='alice' AND room='dummy-13'")==[(1,)])
        terminal.close()

    def test_stop_saves_partial_task_and_allows_new_request(self):
        model=self.model(); terminal=self.app(model=model); terminal.send('@bot start a reply\r')
        eventually(lambda:self.sql("SELECT 1 FROM agent_tasks WHERE partial='partial text received'"))
        terminal.click(100,2)
        eventually(lambda:self.sql("SELECT 1 FROM agent_tasks WHERE state='cancelled'"))
        self.assertTrue(model.disconnected.wait(2)); self.assertTrue(self.sql("SELECT 1 FROM messages WHERE kind='interrupted' AND body='partial text received'"))
        terminal.send('@bot try again\r'); eventually(lambda:self.sql("SELECT 1 FROM agent_tasks WHERE state='completed'"))
        terminal.send('/tasks\r'); terminal.wait_text('Agent activity'); terminal.send('\r'); terminal.wait_text('second request completed'); terminal.send('\x1b'); terminal.send('\x1b')
        terminal.send('\x1bOQ'); terminal.wait_text('Model connection'); before=model.checks; terminal.send('r'); eventually(lambda:model.checks>before); terminal.send('\x1b'); terminal.close()

    def test_crash_recovers_partial_and_pending_approval_without_replay(self):
        marker=self.root/'must-not-exist'
        model=self.model(); terminal=self.app(model=model); terminal.send('@bot partial reply\r')
        eventually(lambda:self.sql("SELECT 1 FROM agent_tasks WHERE partial='partial text received'")); terminal.close(crash=True)
        reopened=self.app(model=model); eventually(lambda:self.sql("SELECT 1 FROM agent_tasks WHERE state='interrupted'"))
        self.assertEqual(model.requests,1); self.assertEqual(self.sql("SELECT COUNT(*) FROM messages WHERE kind='interrupted'")[0][0],1)
        model.mode='tool'; model.command=f"printf forbidden > '{marker}'"
        reopened.send('@bot propose a command\r'); eventually(lambda:self.sql("SELECT 1 FROM agent_tasks WHERE state='awaiting_approval'")); reopened.close(crash=True)
        last=self.app(model=model)
        eventually(lambda:self.sql("SELECT COUNT(*) FROM agent_tasks WHERE state='interrupted'")==[(2,)])
        self.assertEqual(self.sql("SELECT approval FROM agent_tasks ORDER BY id DESC LIMIT 1"),[('cancelled',)])
        self.assertFalse(marker.exists()); self.assertEqual(model.requests,2)
        last.send('/tasks\r'); last.wait_text('interrupted'); last.close()

    def test_quit_waits_for_approved_command_and_records_output(self):
        marker=self.root/'started'
        model=self.model('tool',f"printf started > '{marker}'; sleep 0.5; printf command-completed")
        terminal=self.app(model=model); terminal.send('@bot execute fixture\r')
        eventually(lambda:self.sql("SELECT 1 FROM agent_tasks WHERE state='awaiting_approval'")); terminal.send('y'); eventually(marker.exists)
        terminal.close()
        self.assertEqual(self.sql("SELECT state,approval FROM agent_tasks"),[('cancelled','approved')])
        self.assertTrue(self.sql("SELECT 1 FROM messages WHERE kind='tool' AND body LIKE '%command-completed%'"))

    def test_consistent_backup_and_offline_diagnostics(self):
        self.post('lobby','backup fixture'); destination=self.root/'snapshot.sqlite'
        base=[str(OPTIONS.binary),'--data',str(self.data)]
        subprocess.run([*base,'--backup',str(destination)],cwd=self.root,check=True,capture_output=True)
        with contextlib.closing(sqlite3.connect(destination)) as db, db:
            self.assertEqual(db.execute('PRAGMA integrity_check').fetchone()[0],'ok')
            self.assertEqual(db.execute('SELECT body FROM messages').fetchone()[0],'backup fixture')
        restored=self.root/'restored.sqlite';shutil.copy2(destination,restored)
        history=subprocess.run([str(OPTIONS.binary),'--data',str(restored),'--history','--name','alice'],cwd=self.root,check=True,capture_output=True,text=True)
        self.assertIn('backup fixture',history.stdout)
        self.assertNotEqual(subprocess.run([*base,'--backup',str(destination)],cwd=self.root,capture_output=True).returncode,0)
        probe=socket.socket();probe.bind(('127.0.0.1',0));port=probe.getsockname()[1];probe.close()
        check=subprocess.run([*base,'--doctor','--endpoint',f'http://127.0.0.1:{port}/v1'],cwd=self.root,capture_output=True,text=True)
        self.assertNotEqual(check.returncode,0); report=json.loads(check.stdout); self.assertTrue(report['chat_ready']); self.assertFalse(report['model_ready'])

    def test_launcher_owns_only_the_process_it_starts(self):
        server=self.root/'fixture server'; weights=self.root/'fixture model.gguf';weights.write_text('lifecycle fixture, not a real model')
        server.write_text(f'''#!{sys.executable}
import argparse,http.server,json
p=argparse.ArgumentParser(add_help=False);p.add_argument('--port',type=int);p.add_argument('--alias');a,_=p.parse_known_args()
class H(http.server.BaseHTTPRequestHandler):
 def log_message(self,*args):pass
 def do_GET(self):
  body=json.dumps({{'data':[{{'id':a.alias}}]}}).encode();self.send_response(200);self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
http.server.HTTPServer(('127.0.0.1',a.port),H).serve_forever()
''');server.chmod(0o700)
        probe=socket.socket();probe.bind(('127.0.0.1',0));port=probe.getsockname()[1];probe.close()
        base=[str(OPTIONS.launcher),'--data',str(self.data),'--config',str(self.root/'settings.json')]
        args=[*base,'--model-only','--endpoint',f'http://127.0.0.1:{port}/v1','--model','fixture','--model-path',str(weights),'--server',str(server),'--startup-timeout','5']
        try:
            subprocess.run(args,cwd=self.root,check=True,capture_output=True,timeout=10)
            record=json.loads((self.root/'model-process.json').read_text())
            subprocess.run(args,cwd=self.root,check=True,capture_output=True,timeout=10)
            self.assertEqual(json.loads((self.root/'model-process.json').read_text())['pid'],record['pid'])
            # The ordinary one-command launcher must reuse the running model and enter the TUI.
            terminal=Terminal([*base,'--name','alice','--endpoint',f'http://127.0.0.1:{port}/v1','--model','fixture'],self.root)
            self.terminals.append(terminal);terminal.wait_text('F5 Activity');terminal.send('\x1bOQ');terminal.wait_text('Model connection');terminal.send('\x1b');terminal.close()
            self.assertTrue((self.root/'model-process.json').exists())

        finally:
            subprocess.run([*base,'--stop-model'],cwd=self.root,capture_output=True,timeout=10)
        self.assertFalse((self.root/'model-process.json').exists())
        external=self.model()
        subprocess.run([*base,'--model-only','--endpoint',external.endpoint,'--model','fixture'],cwd=self.root,check=True,capture_output=True,timeout=5)
        subprocess.run([*base,'--stop-model'],cwd=self.root,check=True,capture_output=True,timeout=5)
        self.assertFalse((self.root/'model-process.json').exists())
        check=subprocess.run([str(OPTIONS.binary),'--data',str(self.data),'--doctor','--endpoint',external.endpoint,'--model','fixture'],cwd=self.root,check=True,capture_output=True,text=True)
        self.assertTrue(json.loads(check.stdout)['model_ready'])


if __name__ == '__main__':
    parser=argparse.ArgumentParser()
    parser.add_argument('--binary',type=pathlib.Path,required=True)
    parser.add_argument('--launcher',type=pathlib.Path,required=True)
    OPTIONS=parser.parse_args()
    OPTIONS.binary=OPTIONS.binary.resolve();OPTIONS.launcher=OPTIONS.launcher.resolve()
    unittest.main(argv=[sys.argv[0]],verbosity=2)
