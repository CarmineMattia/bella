#!/usr/bin/env python3
"""Hub protocol and independent-cache terminal tests. No external dependencies."""
import argparse, contextlib, http.server, importlib.util, json, os, pathlib, secrets, sqlite3, subprocess, sys, tempfile, threading, unittest, urllib.error, urllib.request
from alpha_smoke import Terminal, eventually, Model
OPTIONS=None

class Network(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory(prefix='bella-network-');self.root=pathlib.Path(self.temp.name)
        spec=importlib.util.spec_from_file_location('bella_hub',OPTIONS.hub);self.hubmodule=importlib.util.module_from_spec(spec);spec.loader.exec_module(self.hubmodule)
        self.db=self.root/'server.sqlite';self.hubmodule.initialize(self.db)
        self.terminals=[];self.models=[];self.port=0;self.start_hub()
    def start_hub(self):
        self.hub=self.hubmodule.Hub(('127.0.0.1',self.port),self.db,'fixture-invite');self.port=self.hub.server_port
        self.thread=threading.Thread(target=self.hub.serve_forever,daemon=True);self.thread.start();self.url=f'http://127.0.0.1:{self.port}'
    def stop_hub(self):
        self.hub.shutdown();self.hub.server_close();self.thread.join();self.hub=None
    def tearDown(self):
        for terminal in self.terminals:
            if terminal.process.poll() is None:
                with contextlib.suppress(Exception):terminal.close(crash=True)
        for model in self.models:model.close()
        if self.hub:self.stop_hub()
        self.temp.cleanup()
    def call(self,path,body=None,token=None):
        headers={'Content-Type':'application/json'}
        if token:headers['Authorization']='Bearer '+token
        request=urllib.request.Request(self.url+path,data=None if body is None else json.dumps(body).encode(),headers=headers)
        try:
            with urllib.request.urlopen(request,timeout=10) as response:return response.status,json.load(response)
        except urllib.error.HTTPError as error:
            with error:return error.code,json.load(error)
    def register(self,name):
        status,c=self.call('/register',{'username':name,'password':'test-password-123','invite':'fixture-invite'});self.assertEqual(status,200,c);c['url']=self.url;return c
    def post(self,c,p,event_id=None):
        return self.call('/events',{'event_id':event_id or secrets.token_hex(16),'payload':p},c['token'])
    def connect(self,name,credential=None,model=None):
        device=self.root/name;device.mkdir(exist_ok=True)
        if credential:
            path=device/'chat.network.json';path.write_text(json.dumps(credential));path.chmod(0o600)
        command=[str(OPTIONS.binary),'--name',credential['username'] if credential else name,'--data',str(device/'chat.sqlite'),'--network']
        if model:command+=['--endpoint',model.endpoint,'--model','fixture']
        terminal=Terminal(command,self.root);self.terminals.append(terminal)
        if credential:terminal.wait_text('F5 Activity')
        return terminal
    def sql(self,device,sql,params=()):
        files=list((self.root/device).glob('network-*.sqlite'))
        if not files:return []
        with contextlib.closing(sqlite3.connect(files[0])) as db:return db.execute(sql,params).fetchall()
    def test_auth_identity_idempotence_private_rooms_and_bot_ownership(self):
        alice=self.register('alice');bob=self.register('bob')
        self.assertEqual(self.call('/events?after=0')[0],401)
        self.assertEqual(self.call('/login',{'username':'alice','password':'wrong-password'})[0],401)
        self.assertEqual(self.call('/register',{'username':'mallory','password':'test-password','invite':'wrong'})[0],403)
        status,login=self.call('/login',{'username':'alice','password':'test-password-123'});self.assertEqual(status,200);self.assertEqual(login['user_id'],alice['user_id'])
        event=secrets.token_hex(16);message={'type':'message','room':'lobby','author':'bob','kind':'human','body':'From Alice'}
        first=self.post(alice,message,event);self.assertEqual(first[0],200,first);second=self.post(alice,message,event);self.assertEqual(first,second)
        public=self.call('/events?after=0',token=bob['token'])[1]['events'];self.assertEqual(next(e for e in public if e['payload'].get('body')=='From Alice')['payload']['author'],'alice')
        self.assertEqual(self.post(bob,{'type':'room','room':'agent:alice'})[0],403)
        self.assertEqual(self.post(alice,{**message,'room':'agent:alice','body':'Private thought'})[0],200)
        bob_events=self.call('/events?after=0',token=bob['token'])[1];self.assertFalse(any(e['payload'].get('body')=='Private thought' for e in bob_events['events']))
        self.assertGreater(bob_events['cursor'],public[0]['seq'])
        self.assertEqual(self.post(bob,{'type':'message','room':'lobby','author':"alice's @planner",'kind':'bot','body':'Forged'})[0],403)
        profile={'owner':'bob','handle':'planner','name':'Planner','about':'Planning','avatar':[],'instructions':'secret instructions','model':'private-model'}
        self.assertEqual(self.post(alice,{'type':'profile','profile':profile})[0],200)
        self.assertEqual(self.post(alice,{'type':'bot_membership','room':'lobby','handle':'planner','enabled':True})[0],200)
        self.assertEqual(self.post(alice,{'type':'message','room':'lobby','author':"alice's @planner",'kind':'bot','body':'Plan'})[0],200)
        events=self.call('/events?after=0',token=bob['token'])[1]['events'];shared=next(e['payload']['profile'] for e in events if e['payload']['type']=='profile' and e['payload']['profile']['handle']=='planner')
        self.assertEqual(shared['owner'],'alice');self.assertEqual(shared['instructions'],'');self.assertEqual(shared['model'],'')
        self.assertEqual(self.post(alice,{'type':'profile','profile':[]})[0],400)
        self.assertEqual(self.post(alice,{**message,'body':'bad\x1bcontrol'})[0],400)
        self.assertEqual(self.call('/logout',{},alice['token'])[0],200)
        self.assertEqual(self.call('/me',token=alice['token'])[0],401)
    def test_two_independent_caches_sync_profiles_bots_and_offline_queue(self):
        alice=self.register('alice');bob=self.register('bob')
        model=Model('complete');self.models.append(model)
        a=self.connect('device-a',alice,model);b=self.connect('device-b',bob,model)
        a.send('Hello from another computer\r');b.wait_text('Hello from another computer')
        eventually(lambda:self.sql('device-a','SELECT COUNT(*) FROM network_outbox')[0][0]==0)
        self.assertEqual(len(self.sql('device-a',"SELECT id FROM messages WHERE body='Hello from another computer'")),1)
        b.send('/join workshop\r');b.send('A shared new group\r');eventually(lambda:self.sql('device-a',"SELECT room FROM conversations WHERE room='workshop'"))
        a.send('/join workshop\r');a.wait_text('A shared new group')
        a.send('/profile\r');a.wait_text('Your profile');a.send('\x15Alice Online\tSynced profile\x13')
        eventually(lambda:self.sql('device-b',"SELECT data FROM profiles WHERE owner='alice' AND handle=''" ) and json.loads(self.sql('device-b',"SELECT data FROM profiles WHERE owner='alice' AND handle=''" )[0][0])['name']=='Alice Online')
        a.send('/bots\r');a.wait_text('Bots ·');a.send('n');a.wait_text('Create a bot');a.send('planner\t\x15Planner\tShared helper\tMake clear plans\tfixture\t\x13');a.wait_text('@planner');a.send('\x1b');a.send('@planner a test\r')
        b.wait_text('second request completed')
        self.assertEqual(len(model.bodies),1) # Receiving a mention did not run Bob's bot.
        eventually(lambda:self.sql('device-b',"SELECT handle FROM room_bots WHERE owner='alice' AND room='workshop' AND handle='planner'"))
        private='private agent context stays with Alice';a.send('/agent\r');a.send(private+'\r');eventually(lambda:len(model.bodies)==2)
        self.assertFalse(self.sql('device-b','SELECT id FROM messages WHERE body=?',(private,)))
        # Server restart and client restart must preserve and deliver the outbox once.
        self.stop_hub();a.send('/join workshop\r');a.send('/to group\r');a.send('queued while disconnected\r')
        eventually(lambda:self.sql('device-a',"SELECT id FROM network_outbox WHERE json_extract(payload,'$.body')='queued while disconnected'"))
        a.close();self.start_hub();a=self.connect('device-a',alice,model)
        b.wait_text('queued while disconnected')
        eventually(lambda:self.sql('device-a','SELECT COUNT(*) FROM network_outbox WHERE error=\'\'')[0][0]==0,seconds=12)
        self.assertEqual(len(self.sql('device-b',"SELECT id FROM messages WHERE body='queued while disconnected'")),1)
        self.assertFalse((self.root/'device-a/chat.sqlite').exists())
        a.close();b.close()
    def test_lent_bot_answers_guest_mentions_chat_only_after_owner_opt_in(self):
        alice=self.register('alice');bob=self.register('bob')
        model=Model('complete');self.models.append(model)
        a=self.connect('device-a',alice,model);b=self.connect('device-b',bob)
        b.send('/join lending\r');b.send('Group for lending\r');eventually(lambda:self.sql('device-a',"SELECT room FROM conversations WHERE room='lending'"))
        a.send('/join lending\r');a.wait_text('Group for lending')
        a.send('/bots\r');a.wait_text('Bots ·');a.send('n');a.wait_text('Create a bot');a.send('planner\t\x15Planner\tShared helper\tMake clear plans\tfixture\t\x13');a.wait_text('@planner')
        eventually(lambda:self.sql('device-b',"SELECT handle FROM room_bots WHERE owner='alice' AND room='lending' AND handle='planner'"))
        # Without the owner's opt-in a guest mention reaches Alice's device but runs nothing.
        b.send('\x1b');b.send('@planner before lending\r')
        eventually(lambda:self.sql('device-a',"SELECT id FROM messages WHERE body='@planner before lending'"))
        eventually(lambda:self.sql('device-a',"SELECT outcome FROM delegate_requests")==[('not_lent',)])
        self.assertEqual(len(model.bodies),0)
        # Alice opts in from her own bot form (Lend for chat is the last field), still in the Bots dialog.
        a.send('\x1b[B');a.send('e');a.wait_text('Bot profile');a.send('\x1b[Z\x15yes\x13')
        eventually(lambda:json.loads(self.sql('device-a',"SELECT data FROM profiles WHERE owner='alice' AND handle='planner'")[0][0]).get('lend_chat') is True)
        a.send('\x1b')
        b.send('@planner guest question\r');b.wait_text('second request completed')
        self.assertEqual(len(model.bodies),1)
        body=model.bodies[0]
        self.assertNotIn('tools',body)
        self.assertTrue(any('Make clear plans' in m.get('content','') for m in body['messages'] if m['role']=='system'))
        self.assertTrue(any(m['role']=='user' and m['content']=='bob: @planner guest question' for m in body['messages']))
        self.assertFalse(any('Working directory' in m.get('content','') for m in body['messages']))
        self.assertEqual(self.sql('device-b',"SELECT author,kind FROM messages WHERE body='second request completed'"),[("alice's @planner",'bot')])
        self.assertEqual(self.sql('device-a',"SELECT requester,state FROM agent_tasks"),[('bob','completed')])
        # Lending settings stay on Alice's device: Bob's copy of the profile has no opt-in.
        bob_profile=json.loads(self.sql('device-b',"SELECT data FROM profiles WHERE owner='alice' AND handle='planner'")[0][0])
        self.assertFalse(bob_profile.get('lend_chat',False))
        self.assertNotEqual(bob_profile.get('guest_tools'),'shell')
        alice_profile=json.loads(self.sql('device-a',"SELECT data FROM profiles WHERE owner='alice' AND handle='planner'")[0][0])
        self.assertEqual(alice_profile.get('guest_tools','off'),'off')
        a.close();b.close()
    def test_large_history_same_account_bootstrap_and_logout(self):
        alice=self.register('alice')
        for n in range(45):
            status,_=self.post(alice,{'type':'message','room':'lobby','author':'alice','kind':'human','body':f'previous device message {n}'})
            self.assertEqual(status,200)
        self.assertEqual(self.post(alice,{'type':'profile','profile':{'owner':'alice','handle':'','name':'Existing Display Name','about':'Already configured','avatar':[]}})[0],200)
        terminal=self.connect('fresh-device',alice)
        eventually(lambda:self.sql('fresh-device',"SELECT id FROM messages WHERE body='previous device message 44'"))
        terminal.send('/profile\r');terminal.wait_text('Existing Display Name');terminal.send('\x1b')
        self.assertEqual(self.sql('fresh-device','SELECT COUNT(*) FROM messages')[0][0],45)
        self.assertEqual(self.sql('fresh-device',"SELECT COUNT(*) FROM network_outbox WHERE json_extract(payload,'$.type')='profile'")[0][0],0)
        terminal.close()
        command=[str(OPTIONS.binary),'--data',str(self.root/'fresh-device/chat.sqlite'),'--network-logout']
        result=subprocess.run(command,cwd=self.root,capture_output=True,text=True,timeout=10)
        self.assertEqual(result.returncode,0,result.stderr);self.assertFalse((self.root/'fresh-device/chat.network.json').exists())
        self.assertEqual(self.call('/me',token=alice['token'])[0],401)

    def test_real_terminal_registration_masking_and_saved_session(self):
        terminal=self.connect('newuser');terminal.wait_text('Connect to Bella hub')
        terminal.send('\x15'+self.url+'\tnewuser\ttest-password-123\tfixture-invite')
        self.assertNotIn('test-password-123',terminal.screen());self.assertNotIn('fixture-invite',terminal.screen())
        terminal.send('\x12');terminal.wait_text('F5 Activity')
        credential_path=self.root/'newuser/chat.network.json';saved=json.loads(credential_path.read_text());self.assertEqual(saved['username'],'newuser');self.assertNotIn('password',saved);self.assertEqual(credential_path.stat().st_mode&0o777,0o600)
        terminal.send('registered through terminal\r');eventually(lambda:self.sql('newuser','SELECT COUNT(*) FROM network_receipts')[0][0]==1)
        terminal.close();terminal=self.connect('newuser',saved);terminal.wait_text('registered through terminal');terminal.close()

if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--binary',required=True,type=pathlib.Path);parser.add_argument('--hub',type=pathlib.Path,default=pathlib.Path(__file__).resolve().parents[1]/'server/hub.py');OPTIONS=parser.parse_args();OPTIONS.binary=OPTIONS.binary.resolve();OPTIONS.hub=OPTIONS.hub.resolve()
    unittest.main(argv=[sys.argv[0]],verbosity=2)
