#!/usr/bin/env python3
"""Term Social shared hub. Python standard library only; see docs/NETWORK.md."""
import argparse, contextlib, datetime, hashlib, hmac, http.server, json, os, pathlib, re, secrets, socket, sqlite3, ssl, threading, time, urllib.parse, uuid, unicodedata
VERSION = 1
MAX_BODY = 256 * 1024

def printable(value, limit, empty=True):
    return isinstance(value,str) and len(value)<=limit and (empty or bool(value.strip())) and all(unicodedata.category(c)!='Cc' for c in value)

def room_valid(room, username):
    return printable(room,64,False) and (not room.startswith('agent:') or room=='agent:'+username)

def handle_valid(handle): return isinstance(handle,str) and re.fullmatch('[a-z][a-z0-9_]{0,23}',handle) is not None

def initialize(path):
    path.parent.mkdir(parents=True,exist_ok=True)
    with contextlib.closing(sqlite3.connect(path)) as db, db:
        db.executescript('''PRAGMA journal_mode=WAL;
        CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY,value TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS accounts(id TEXT PRIMARY KEY,username TEXT UNIQUE NOT NULL,salt BLOB NOT NULL,password BLOB NOT NULL);
        CREATE TABLE IF NOT EXISTS sessions(token TEXT PRIMARY KEY,account TEXT NOT NULL,expires INTEGER NOT NULL);
        CREATE TABLE IF NOT EXISTS events(seq INTEGER PRIMARY KEY,account TEXT NOT NULL,event_id TEXT NOT NULL,payload TEXT NOT NULL,created TEXT NOT NULL,UNIQUE(account,event_id));
        CREATE TABLE IF NOT EXISTS bots(owner TEXT NOT NULL,handle TEXT NOT NULL,PRIMARY KEY(owner,handle));
        CREATE TABLE IF NOT EXISTS room_bots(owner TEXT NOT NULL,room TEXT NOT NULL,handle TEXT NOT NULL,PRIMARY KEY(owner,room,handle));
        ''')
        db.execute('INSERT OR IGNORE INTO meta VALUES(?,?)',('server_id',uuid.uuid4().hex))
    os.chmod(path,0o600)

class Error(Exception):
    def __init__(self,status,message): self.status=status;self.message=message

class Hub(http.server.ThreadingHTTPServer):
    daemon_threads=True
    allow_reuse_address=True
    def __init__(self,address,path,invite):
        self.path=path;self.invite=invite;self.slots=threading.BoundedSemaphore(16)
        super().__init__(address,Handler)
    def process_request(self,request,address):
        if not self.slots.acquire(False): self.shutdown_request(request);return
        try: super().process_request(request,address)
        except BaseException: self.slots.release();raise
    def process_request_thread(self,request,address):
        try: super().process_request_thread(request,address)
        finally:self.slots.release()

class Handler(http.server.BaseHTTPRequestHandler):
    protocol_version='HTTP/1.0'
    server_version='TermSocialHub/1'
    def setup(self):
        super().setup();self.connection.settimeout(8)
    def log_message(self,*args): pass # Never log bearer tokens, invitations, passwords or message bodies.
    def send_json(self,status,data):
        body=json.dumps(data,separators=(',',':'),ensure_ascii=False).encode()
        self.send_response(status);self.send_header('Content-Type','application/json; charset=utf-8');self.send_header('Content-Length',str(len(body)));self.send_header('Cache-Control','no-store');self.end_headers();self.wfile.write(body)
    def body(self):
        try:length=int(self.headers.get('Content-Length','0'))
        except ValueError:raise Error(400,'Invalid body length')
        if not 0<length<=MAX_BODY:raise Error(413,'Request too large or empty')
        if self.headers.get('Transfer-Encoding'):raise Error(400,'Chunked bodies are not supported')
        try:data=json.loads(self.rfile.read(length))
        except (ValueError,UnicodeError):raise Error(400,'Invalid JSON')
        if not isinstance(data,dict):raise Error(400,'Expected a JSON object')
        return data
    def account(self,db):
        auth=self.headers.get('Authorization','')
        if not auth.startswith('Bearer ') or len(auth)>256:raise Error(401,'Sign in again')
        digest=hashlib.sha256(auth[7:].encode()).hexdigest()
        row=db.execute('SELECT a.id,a.username FROM sessions s JOIN accounts a ON a.id=s.account WHERE s.token=? AND s.expires>?',(digest,int(time.time()))).fetchone()
        if row is None:raise Error(401,'Sign in again')
        return row
    def serve(self):
        try:
            with contextlib.closing(sqlite3.connect(self.server.path,timeout=5)) as db, db:
                result=self.route(db)
            self.send_json(200,result)
        except Error as e:self.send_json(e.status,{'error':e.message})
        except (BrokenPipeError,ConnectionResetError,socket.timeout):pass
        except sqlite3.OperationalError as error:
            self.send_json(503 if 'locked' in str(error) or 'busy' in str(error) else 400,{'error':'Database temporarily unavailable'})
        except (sqlite3.Error,ValueError,TypeError,KeyError,OverflowError):self.send_json(400,{'error':'Invalid request or database busy'})
    do_GET=serve
    do_POST=serve
    def route(self,db):
        url=urllib.parse.urlsplit(self.path);route=url.path
        server_id=db.execute("SELECT value FROM meta WHERE key='server_id'").fetchone()[0]
        if self.command=='GET' and route=='/health':return {'protocol':VERSION,'server_id':server_id}
        if self.command=='POST' and route in ('/register','/login'):
            data=self.body();username=data.get('username');password=data.get('password')
            if not isinstance(username,str) or not re.fullmatch('[a-z][a-z0-9_]{0,31}',username):raise Error(400,'Account ID: 1–32 lowercase letters, digits or underscores; start with a letter')
            if not isinstance(password,str) or not 8<=len(password.encode())<=256:raise Error(400,'Password must be 8–256 bytes')
            row=db.execute('SELECT id,salt,password FROM accounts WHERE username=?',(username,)).fetchone()
            if route=='/register':
                invite=data.get('invite','')
                if not isinstance(invite,str) or not hmac.compare_digest(invite.encode(),self.server.invite.encode()):raise Error(403,'Invalid registration invite')
                if row:raise Error(409,'Account ID already registered')
                salt=secrets.token_bytes(16);account_id=uuid.uuid4().hex
                digest=hashlib.pbkdf2_hmac('sha256',password.encode(),salt,600_000)
                db.execute('INSERT INTO accounts VALUES(?,?,?,?)',(account_id,username,salt,digest))
                created=datetime.datetime.now(datetime.timezone.utc).strftime('%Y-%m-%d %H:%M:%S')
                for handle,name in [('',username),('bot',(username+"'s bot")[:32])]:
                    profile={'owner':username,'handle':handle,'name':name,'about':'','instructions':'','model':'','avatar':[]}
                    db.execute('INSERT INTO events(account,event_id,payload,created) VALUES(?,?,?,?)',(account_id,secrets.token_hex(16),json.dumps({'type':'profile','profile':profile}),created))
                db.execute('INSERT INTO bots VALUES(?,?)',(username,'bot'))
            else:
                salt=row[1] if row else bytes(16)
                digest=hashlib.pbkdf2_hmac('sha256',password.encode(),salt,600_000)
                if row is None or not hmac.compare_digest(digest,row[2]):raise Error(401,'Incorrect account ID or password')
                account_id=row[0]
            token=secrets.token_urlsafe(32)
            db.execute('DELETE FROM sessions WHERE expires<=?',(int(time.time()),))
            db.execute('INSERT INTO sessions VALUES(?,?,?)',(hashlib.sha256(token.encode()).hexdigest(),account_id,int(time.time())+30*86400))
            return {'protocol':VERSION,'server_id':server_id,'user_id':account_id,'username':username,'token':token}
        account_id,username=self.account(db)
        if self.command=='GET' and route=='/me':return {'protocol':VERSION,'server_id':server_id,'user_id':account_id,'username':username}
        if self.command=='POST' and route=='/logout':
            db.execute('DELETE FROM sessions WHERE token=?',(hashlib.sha256(self.headers['Authorization'][7:].encode()).hexdigest(),));return {'ok':True}
        if self.command=='GET' and route=='/events':
            try:after=int(urllib.parse.parse_qs(url.query).get('after',['0'])[0])
            except ValueError:raise Error(400,'Invalid cursor')
            if not 0<=after<=2**63-1:raise Error(400,'Invalid cursor')
            rows=db.execute('SELECT seq,account,event_id,payload,created FROM events WHERE seq>? ORDER BY seq LIMIT 20',(after,)).fetchall()
            events=[]
            for seq,actor,event_id,payload,created in rows:
                payload=json.loads(payload)
                room=payload.get('room','')
                if room.startswith('agent:') and actor!=account_id:continue
                events.append({'seq':seq,'account_id':actor,'event_id':event_id,'payload':payload,'created':created})
            return {'events':events,'cursor':rows[-1][0] if rows else after,'more':len(rows)==20}
        if self.command=='POST' and route=='/events':
            data=self.body();event_id=data.get('event_id');payload=data.get('payload')
            if not isinstance(event_id,str) or not re.fullmatch('[a-f0-9]{32}',event_id) or not isinstance(payload,dict):raise Error(400,'Invalid event envelope')
            # Receipt lookup precedes validation: a retry remains valid after membership changed.
            old=db.execute('SELECT seq FROM events WHERE account=? AND event_id=?',(account_id,event_id)).fetchone()
            if old:return {'seq':old[0]}
            payload=self.validate_event(db,username,payload)
            created=datetime.datetime.now(datetime.timezone.utc).strftime('%Y-%m-%d %H:%M:%S')
            cursor=db.execute('INSERT INTO events(account,event_id,payload,created) VALUES(?,?,?,?)',(account_id,event_id,json.dumps(payload,separators=(',',':')),created))
            return {'seq':cursor.lastrowid}
        raise Error(404,'Unknown endpoint')
    def validate_event(self,db,username,p):
        kind=p.get('type')
        if kind=='profile':
            data=p.get('profile',{})
            if not isinstance(data,dict):raise Error(400,'Invalid profile')
            handle=data.get('handle','')
            if not isinstance(data,dict) or (handle and not handle_valid(handle)):raise Error(400,'Invalid profile')
            name=data.get('name');about=data.get('about','');avatar=data.get('avatar',[])
            if not printable(name,32,False) or not printable(about,160):raise Error(400,'Invalid profile text')
            if not isinstance(avatar,list) or len(avatar) not in (0,576) or any(not isinstance(pixel,list) or len(pixel)!=3 or any(type(c) is not int or not 0<=c<=255 for c in pixel) for pixel in avatar):raise Error(400,'Invalid avatar')
            if handle:db.execute('INSERT OR IGNORE INTO bots VALUES(?,?)',(username,handle))
            return {'type':kind,'profile':{'owner':username,'handle':handle,'name':name,'about':about,'avatar':avatar,'instructions':'','model':''}}
        room=p.get('room')
        if not room_valid(room,username):raise Error(403,'Invalid room or private agent room')
        if kind=='room':return {'type':kind,'room':room}
        if kind=='bot_membership':
            handle=p.get('handle');enabled=p.get('enabled')
            if not handle_valid(handle) or type(enabled) is not bool or not db.execute('SELECT 1 FROM bots WHERE owner=? AND handle=?',(username,handle)).fetchone():raise Error(403,'Bot must belong to your account')
            if enabled:db.execute('INSERT OR IGNORE INTO room_bots VALUES(?,?,?)',(username,room,handle))
            else:db.execute('DELETE FROM room_bots WHERE owner=? AND room=? AND handle=?',(username,room,handle))
            return {'type':kind,'owner':username,'room':room,'handle':handle,'enabled':enabled}
        if kind!='message':raise Error(400,'Unknown event type')
        body=p.get('body');message_kind=p.get('kind');author=p.get('author')
        if not isinstance(body,str) or not body.strip() or len(body.encode())>64*1024 or any(unicodedata.category(c)=='Cc' and c not in '\n\t' for c in body):raise Error(400,'Message must be 1–65536 bytes without terminal controls')
        if message_kind=='human':author=username
        elif message_kind in ('bot','interrupted'):
            handle='bot' if author==username+"'s bot" else author.removeprefix(username+"'s @") if isinstance(author,str) and author.startswith(username+"'s @") else ''
            if not handle_valid(handle) or not db.execute('SELECT 1 FROM bots WHERE owner=? AND handle=?',(username,handle)).fetchone():raise Error(403,'Bot must belong to your account')
            if handle!='bot' and not db.execute('SELECT 1 FROM room_bots WHERE owner=? AND room=? AND handle=?',(username,room,handle)).fetchone():raise Error(403,'Bot is not in this chat')
        elif message_kind in ('tool','system'):
            author='tools' if message_kind=='tool' else 'system';body=f'[{username}] '+body
        else:raise Error(400,'Invalid message kind')
        return {'type':kind,'room':room,'author':author,'body':body,'kind':message_kind}

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--data',type=pathlib.Path,default=pathlib.Path('.data/hub.sqlite'))
    parser.add_argument('--bind',default='127.0.0.1');parser.add_argument('--port',type=int,default=8787)
    parser.add_argument('--tls-cert');parser.add_argument('--tls-key')
    args=parser.parse_args();path=args.data.expanduser().resolve();initialize(path)
    invite_path=path.with_suffix('.invite')
    if not invite_path.exists():
        fd=os.open(invite_path,os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600)
        with os.fdopen(fd,'w') as file:file.write(secrets.token_urlsafe(24)+'\n')
    hub=Hub((args.bind,args.port),path,invite_path.read_text().strip())
    if bool(args.tls_cert)!=bool(args.tls_key):parser.error('Supply both --tls-cert and --tls-key')
    if args.tls_cert:
        context=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER);context.minimum_version=ssl.TLSVersion.TLSv1_2;context.load_cert_chain(args.tls_cert,args.tls_key);hub.socket=context.wrap_socket(hub.socket,server_side=True)
    print(f'Term Social hub listening on {args.bind}:{hub.server_port}. Registration invite: {invite_path}',flush=True)
    try:hub.serve_forever()
    except KeyboardInterrupt:pass
    finally:hub.server_close()
if __name__=='__main__':main()
