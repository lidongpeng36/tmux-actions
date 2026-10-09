"""Real native search, argument transport, and sidebar layout/lifecycle tests."""
import contextlib,json,os,pathlib,subprocess,tempfile,time
ROOT=pathlib.Path(__file__).resolve().parent.parent
BIN=pathlib.Path(os.environ.get('TMUX_ACTIONS_BIN',ROOT/'target/release/tmux-actions')).resolve()
def wait(check,seconds=6):
 end=time.monotonic()+seconds
 while time.monotonic()<end:
  if check():return
  time.sleep(.03)
 raise AssertionError('condition timed out')
def alive(pid):
 try:os.kill(pid,0);return True
 except ProcessLookupError:return False

def main():
 with tempfile.TemporaryDirectory(prefix='tmux-actions-tests-') as temp:
  root=pathlib.Path(temp);socket=str(root/'socket');folder=root/'folder with spaces';folder.mkdir();file=folder/"日本語 'quote $(inert).txt";file.write_text('hello')
  log=root/'open.jsonl';mock=root/'open mock.py';mock.write_text('#!/usr/bin/env python3\nimport json,sys\nwith open('+repr(str(log))+',"a") as f:f.write(json.dumps(sys.argv[1:])+"\\n")\n');mock.chmod(0o755)
  pager=root/'pager.py';pagerlog=root/'pager.json';pager.write_text('#!/usr/bin/env python3\nimport os,sys,time,json\ndata=sys.stdin.read()\nopen('+repr(str(pagerlog))+',"w").write(json.dumps({"pid":os.getpid(),"data":data}))\ntime.sleep(120)\n');pager.chmod(0o755)
  producer=root/'producer.py';url='https://example.com/'+('segment/'*20)+'?q=日本語#fragment';producer.write_text('import time\nprint("alpha 1234 beta",flush=True)\nprint('+repr(url)+',flush=True)\ntime.sleep(120)\n')
  def t(*args):return subprocess.check_output(['tmux','-S',socket,*args],text=True,stderr=subprocess.STDOUT,timeout=8).rstrip('\n')
  def action(mode,*extra,text=None):return subprocess.run([str(BIN),mode,'--socket',socket,'--pane',pane,'--launcher',str(ROOT/'scripts/start.sh'),*extra],input=text,text=True,capture_output=True,timeout=8)
  t('-f','/dev/null','new-session','-d','-c',str(folder),'python3',str(producer));pane=t('display','-p','#{pane_id}');window=t('display','-p','#{window_id}')
  try:
   t('set','-g','@tmux-actions-width-cache',str(root/'widths.json'));t('set','-g','set-clipboard','off');t('set','-g','mode-keys','vi');t('set','-g','@tmux-actions-bin',str(BIN));t('set','-g','@tmux-actions-opener',json.dumps([str(mock)]));t('set','-g','@tmux-actions-editor',json.dumps([str(mock)]));t('set','-g','@tmux-actions-pager',json.dumps([str(pager)]))
   result=action('configure');assert result.returncode==0,result.stderr
   wait(lambda:url in t('capture-pane','-p','-J'))
   for pattern,wanted in [('[[:digit:]]+','1234'),('https?://[^[:space:]]+',url)]:
    t('copy-mode');t('send-keys','-X','search-backward',pattern);t('send-keys','-X','copy-selection-and-cancel');assert t('show-buffer')==wanted
   result=action('open',text=str(file));assert result.returncode==0,result.stderr
   raw='https://example.com/a?x=%24%28inert%29#fragment';result=action('open',text=raw);assert result.returncode==0,result.stderr
   query="日本語 & 'quote $(not-a-command)";result=action('web',text=query);assert result.returncode==0,result.stderr
   rows=[json.loads(s) for s in log.read_text().splitlines()];assert rows[0]==[str(file)];assert rows[1]==[raw];assert '%26' in rows[2][0] and '%24%28' in rows[2][0]
   result=action('edit',text=str(file)+':42:3');assert result.returncode==0,result.stderr
   wait(lambda:len(log.read_text().splitlines())==4);assert json.loads(log.read_text().splitlines()[3])==['+42','--',str(file)]
   hashfile=folder/'literal#(touch marker).txt';hashfile.write_text('safe')
   result=action('edit',text=str(hashfile));assert result.returncode==0,result.stderr
   wait(lambda:len(log.read_text().splitlines())==5)
   assert json.loads(log.read_text().splitlines()[4])==['--',str(hashfile)]
   assert not (folder/'marker').exists(),'tmux formatted selected text as a command'
   t('select-window','-t',window);t('select-pane','-t',pane)
   layout=t('display','-p','-t',pane,'#{window_layout}');before=t('list-panes','-t',window,'-F','#{pane_id}').splitlines()
   result=action('sidebar');assert result.returncode==0,result.stderr
   wait(lambda:pagerlog.exists());view=json.loads(pagerlog.read_text());assert "日本語 'quote $(inert).txt" in view['data']
   side=json.loads(t('show-option','-wqv','-t',window,'@tmux-actions-sidebar'))['pane'];assert t('display','-p','-t',window,'#{pane_id}')==pane
   t('resize-pane','-t',side,'-x','30');result=action('sidebar');assert result.returncode==0,result.stderr
   wait(lambda:not alive(view['pid']));assert t('list-panes','-t',window,'-F','#{pane_id}').splitlines()==before
   assert t('display','-p','-t',pane,'#{window_layout}')==layout
   result=action('sidebar','--focus');assert result.returncode==0,result.stderr
   side=json.loads(t('show-option','-wqv','-t',window,'@tmux-actions-sidebar'))['pane'];assert t('display','-p','-t',window,'#{pane_id}')==side
   assert t('display','-p','-t',side,'#{pane_width}')=='30'
   result=action('sidebar');assert result.returncode==0,result.stderr
   subprocess.run(['git','init',str(folder)],check=True,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
   changed='changed[1].rs';(folder/changed).write_text('modified')
   git_producer=root/'git_producer.py';git_producer.write_text('import time\nprint('+repr(changed)+',flush=True)\ntime.sleep(120)\n')
   t('respawn-pane','-k','-t',pane,'-c',str(folder),'python3',str(git_producer))
   wait(lambda:changed in t('capture-pane','-p','-t',pane))
   result=action('git-search');assert result.returncode==0,result.stderr
   assert t('display','-p','#{search_match}')==changed
   t('send-keys','-X','copy-selection-and-cancel');assert t('show-buffer')==changed
   print(json.dumps({'git_filename_regex_escape':True,'native_regex_copy' :True,'wrapped_unicode_url':True,'literal_opener_args':True,'encoded_web_query':True,'editor_file_line':True,'sidebar_layout_focus_width':True,'pager_cleanup':True}))
  finally:
   with contextlib.suppress(subprocess.CalledProcessError):t('kill-server')
if __name__=='__main__':main()
