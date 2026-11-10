#!/usr/bin/env python3
"""Opt-in real-model acceptance for an extracted alpha. Approves only printf alpha-release-ok."""
import argparse, contextlib, hashlib, json, pathlib, shlex, sqlite3, subprocess, tempfile, time
from alpha_smoke import Terminal, eventually

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--launcher',type=pathlib.Path,required=True)
    parser.add_argument('--endpoint',default='http://127.0.0.1:8080/v1')
    parser.add_argument('--model',default='Qwen3.6-35B-A3B-UD-Q4_K_XL')
    args=parser.parse_args();launcher=args.launcher.resolve()
    with tempfile.TemporaryDirectory(prefix='bella-live-alpha-') as directory:
        root=pathlib.Path(directory);data=root/'chat.sqlite'
        def query(sql):
            with contextlib.closing(sqlite3.connect(data)) as db:return db.execute(sql).fetchall()
        terminal=Terminal([str(launcher),'--name','alpha-release-tester','--data',str(data),'--config',str(root/'settings.json'),'--endpoint',args.endpoint,'--model',args.model],root)
        try:
            terminal.wait_text('F5 Activity')
            terminal.send('@bot Use the shell tool to execute exactly `printf alpha-release-ok`. After the tool returns, reply with exactly alpha-release-ok.\r')
            proposed=eventually(lambda:query("SELECT command FROM agent_tasks WHERE state='awaiting_approval'"),seconds=90)[0][0]
            if shlex.split(proposed)!=['printf','alpha-release-ok']:
                terminal.send('n');raise AssertionError(f'Model proposed a different command; denied: {proposed!r}')
            terminal.send('y')
            eventually(lambda:query("SELECT 1 FROM agent_tasks WHERE state='completed'"),seconds=90)
            assert query("SELECT state,approval FROM agent_tasks")==[('completed','approved')]
            assert query("SELECT 1 FROM messages WHERE kind='tool' AND body LIKE '%alpha-release-ok%'")
            assert query("SELECT 1 FROM messages WHERE kind='bot' AND body LIKE '%alpha-release-ok%'")
            terminal.send('/tasks\r');terminal.wait_text('Agent activity');terminal.send('\r');terminal.wait_text('alpha-release-ok');terminal.send('\x1b');terminal.send('\x1b');terminal.close()
            version=subprocess.check_output([str(launcher),'--version'],text=True).strip()
            print(json.dumps({'result':'passed','version':version,'model':args.model,'endpoint':args.endpoint,'approved_command':proposed,'checks':['normal launcher with real model reuse','real streamed completion','exact harmless owner-approved shell tool','durable completed task and approval','tool output in chat','activity details','graceful exit']},indent=2))
        finally:
            if terminal.process.poll() is None:terminal.close(crash=True)
if __name__=='__main__':main()
