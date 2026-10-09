"""Drive actual prefix and copy-mode bindings through a PTY client."""
import contextlib,fcntl,json,os,pathlib,pty,select,struct,subprocess,tempfile,termios,time
from actions import BIN,ROOT,wait
with tempfile.TemporaryDirectory(prefix='tmux-actions-keys-') as temp:
 d=pathlib.Path(temp);socket=str(d/'socket');producer=d/'producer.py';url='https://example.com/path?x=1#fragment'
 producer.write_text('import time\nprint('+repr(url+'，中文说明。')+',flush=True)\ntime.sleep(120)\n')
 def t(*a):return subprocess.check_output(['tmux','-S',socket,*a],text=True,stderr=subprocess.STDOUT,timeout=8).rstrip('\n')
 t('-f','/dev/null','new-session','-d','python3',str(producer));t('set','-g','prefix','C-z');t('set','-g','mode-keys','vi');t('set','-g','set-clipboard','off');t('set','-g','@tmux-actions-bin',str(BIN));t('set','-g','@tmux-actions-width-cache',str(d/'widths.json'))
 master,slave=pty.openpty();fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',24,180,0,0))
 client=subprocess.Popen(['tmux','-S',socket,'attach'],stdin=slave,stdout=slave,stderr=slave,env={**os.environ,'TERM':'xterm-256color'});os.close(slave)
 try:
  for _ in range(3):t('run-shell',str(ROOT/'tmux-actions.tmux'))
  wait(lambda:t('show-option','-gqv','@tmux-actions-version')==(ROOT/'VERSION').read_text().strip())
  assert t('show-hooks','-g','client-attached').count('tmux-actions-load-hook')==1
  wait(lambda:url in t('capture-pane','-p'))
  os.write(master,b'\x1a\x15');wait(lambda:t('display','-p','#{search_match}')==url)
  os.write(master,b'\r');wait(lambda:t('display','-p','#{pane_in_mode}')=='0');assert t('show-buffer')==url
  os.write(master,b'\x1a/');time.sleep(.15);os.write(master,'https?://[^[:space:]，。]+\r'.encode());wait(lambda:t('display','-p','#{search_match}')==url)
  os.write(master,b'\r');wait(lambda:t('display','-p','#{pane_in_mode}')=='0')
  os.write(master,b'\x1a\x7f')
  wait(lambda:bool(t('show-option','-wqv','@tmux-actions-sidebar')))
  side=json.loads(t('show-option','-wqv','@tmux-actions-sidebar'))['pane']
  wait(lambda:t('display','-p','#{pane_id}')==side)
  wait(lambda:'src/' in t('capture-pane','-p','-t',side))
  os.write(master,b'q')
  wait(lambda:t('show-option','-wqv','@tmux-actions-sidebar')=='')
  assert len(t('list-panes','-F','#{pane_id}').splitlines())==1
  print(json.dumps({'real_less_sidebar_quit':True,'real_prefix_url_search' :True,'regex_prompt':True,'enter_copies_match':True,'reload_single_attach_hook':True}))
 finally:
  with contextlib.suppress(subprocess.CalledProcessError):t('kill-server')
  client.wait(timeout=3);os.close(master)
