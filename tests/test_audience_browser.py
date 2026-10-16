"""Exercise the shipped audience script with Node's VM and deterministic DOM/WS fakes.

Run with: python3 -m unittest tests/test_audience_browser.py
Node is needed for JavaScript execution; no browser, server, or temporary scripts
are used. Each case loads the actual shared module from assets/audience/audience.js.
"""

from pathlib import Path
import shutil
import subprocess
import unittest


REPO = Path(__file__).resolve().parents[1]
NODE = shutil.which("node")
HARNESS = r"""
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const html = fs.readFileSync('assets/audience/index.html', 'utf8');
const script = fs.readFileSync('assets/audience/audience.js', 'utf8');
assert.match(html, /<script type="module" src="\/assets\/audience.js"><\/script>/);

function client(hash = '#k=' + 'A'.repeat(43), storage = new Map(), blockedStorage = false, pathname = '/join') {
  const elements = new Map();
  for (const id of ['status', 'retry', 'feedback', 'comments', 'body', 'handle', 'send-comment']) {
    elements.set(id, {
      addEventListener(type, handler) { this['on' + type] = handler; },
      value: id === 'handle' ? 'Anonymous' : '',
      textContent: '', disabled: true, hidden: id === 'retry', error: '',
      setCustomValidity(message) { this.error = message; },
      get validity() { return {valid: this.error === ''}; },
      set innerHTML(value) { throw new Error('audience input must remain plain text'); }
    });
  }
  const reactions = ['thumbs_up', 'heart', 'applause', 'laugh', 'question'].map(kind => ({
    dataset: {kind}, disabled: true, addEventListener(type, handler) { this['on' + type] = handler; }
  }));
  let validityReports = 0;
  elements.get('comments').reportValidity = () => { validityReports++; };
  elements.get('comments').requestSubmit = () => {
    elements.get('comments').onsubmit({preventDefault() {}});
  };
  const sockets = [];
  class WebSocket {
    static OPEN = 1;
    constructor(url) { this.url = url; this.readyState = 0; this.sent = []; sockets.push(this); }
    addEventListener(type, handler) { this['on' + type] = handler; }
    send(text) { assert.equal(this.readyState, WebSocket.OPEN); this.sent.push(JSON.parse(text)); }
    close() { this.readyState = 3; if (this.onclose) this.onclose(); }
  }
  const timers = new Map();
  let timerId = 0;
  const replaced = [];
  const context = vm.createContext({
    document: {
      querySelector: selector => {
        const ids = {'#connection-status':'status', '#retry':'retry', '#feedback':'feedback', 'form':'comments', '#message':'body', '#nickname':'handle', '#send-comment':'send-comment'};
        assert(ids[selector]); return elements.get(ids[selector]);
      },
      querySelectorAll: selector => { assert.equal(selector, '[data-kind]'); return reactions; }
    },
    location: {hash, host: '127.0.0.1:4321', pathname, protocol: 'http:', href: 'http://127.0.0.1:4321' + pathname},
    sessionStorage: {
      setItem(key, value) { if (blockedStorage) throw new Error('Storage unavailable'); storage.set(key, value); },
      getItem(key) { if (blockedStorage) throw new Error('Storage unavailable'); return storage.get(key) ?? null; },
      removeItem(key) { if (blockedStorage) throw new Error('Storage unavailable'); storage.delete(key); }
    },
    history: {replaceState: (...args) => replaced.push(args)},
    URL, URLSearchParams, TextEncoder, WebSocket,
    setTimeout: (callback, delay) => { timers.set(++timerId, {callback, delay}); return timerId; },
    clearTimeout: id => timers.delete(id)
  });
  vm.runInContext(script.replace('export function mountAudience', 'function mountAudience') + '\nmountAudience(globalThis);', context, {timeout: 1000});
  const socket = () => sockets.at(-1);
  function open() { socket().readyState = WebSocket.OPEN; socket().onopen(); }
  function receive(message, target = socket()) { target.onmessage({data: JSON.stringify(message)}); }
  function welcome(capabilities = ['reactions', 'comments'], available = true) {
    open(); receive({protocol: 'qp.audience', version: '1-draft.1', type: 'welcome', connection_id: 'a1', capabilities, available, limits: {max_message_bytes:4096, request_cache_ms:30000}});
  }
  function submit(body = 'Hello', handle = 'Anonymous') {
    elements.get('body').value = body;
    elements.get('handle').value = handle;
    let prevented = false;
    elements.get('comments').onsubmit({preventDefault() { prevented = true; }});
    assert(prevented);
  }
  function result(status, request_id = socket().sent.at(-1).request_id) {
    receive({protocol: 'qp.audience', version: '1-draft.1', type: 'result', request_id, status});
  }
  return {
    elements, reactions, sockets, socket, open, receive, welcome, submit, result,
    timers, replaced, storage, validityReports: () => validityReports
  };
}
"""


@unittest.skipUnless(NODE, "Node is required to execute the audience JavaScript")
class AudienceBrowserTests(unittest.TestCase):
    def run_js(self, case):
        completed = subprocess.run(
            [NODE, "-"], input=HARNESS + "\n" + case, cwd=REPO,
            capture_output=True, text=True, encoding="utf-8", timeout=3,
        )
        self.assertEqual(completed.returncode, 0, completed.stderr)

    def test_authentication_removes_fragment_and_waits_for_capabilities(self):
        self.run_js(r"""
const c = client();
assert.equal(c.socket().url, 'ws://127.0.0.1:4321/ws/audience/join');
assert.equal(JSON.stringify(c.replaced), JSON.stringify([[{}, '', '/join']]));
c.open();
assert.deepEqual(c.socket().sent, [{protocol: 'qp.audience', version: '1-draft.1', type: 'hello', token: 'A'.repeat(43)}]);
assert(c.reactions.every(button => button.disabled));
assert(c.elements.get('send-comment').disabled);
c.receive({protocol: 'qp.audience', version: '1-draft.1', type: 'welcome', connection_id: 'a1', capabilities: ['reactions', 'comments'], available: true, limits: {max_message_bytes:4096, request_cache_ms:30000}});
assert(c.reactions.every(button => !button.disabled));
assert.equal(c.elements.get('send-comment').disabled, false);
""")

    def test_missing_join_token_does_not_connect_or_submit(self):
        self.run_js(r"""
for (const hash of ['', '#k=']) {
  const c = client(hash);
  assert.equal(c.sockets.length, 0);
  c.submit();
  assert.equal(c.sockets.length, 0);
  assert.match(c.elements.get('status').textContent, /complete join link/);
  assert(c.elements.get('send-comment').disabled);
}
""")

    def test_reload_recovers_tab_token_and_new_join_replaces_it(self):
        self.run_js(r"""
const storage = new Map();
const first = client('#k=' + 'A'.repeat(43), storage); first.welcome(); first.socket().close();
const reloaded = client('', storage); reloaded.welcome();
assert.equal(reloaded.socket().sent[0].token, 'A'.repeat(43));
reloaded.submit('After reload');
assert.equal(reloaded.socket().sent[1].body, 'After reload');
const next = client('#k=' + 'B'.repeat(43), storage); next.welcome();
assert.equal(next.socket().sent[0].token, 'B'.repeat(43));
const again = client('', storage); again.welcome();
assert.equal(again.socket().sent[0].token, 'B'.repeat(43));
assert.equal(client('', storage, false, '/different-join').sockets.length, 0);
assert.equal(client('').sockets.length, 0, 'another tab has no retained credential');
""")

    def test_unavailable_tab_storage_keeps_join_fragment_reloadable(self):
        self.run_js(r"""
const c = client('#k=' + 'A'.repeat(43), new Map(), true); c.welcome();
assert.deepEqual(c.replaced, []);
assert.equal(c.socket().sent[0].token, 'A'.repeat(43));
const missing = client('', new Map(), true);
assert.equal(missing.sockets.length, 0);
""")

    def test_enter_sends_once_but_shift_enter_and_composition_do_not(self):
        self.run_js(r"""
const c = client(); c.welcome();
c.elements.get('body').value = 'Line one\nLine two';
function key(overrides = {}) {
  let prevented = false;
  c.elements.get('body').onkeydown({key: 'Enter', shiftKey: false, isComposing: false,
    keyCode: 13, ...overrides, preventDefault() { prevented = true; }});
  return prevented;
}
for (const modifiers of [{shiftKey: true}, {isComposing: true}, {keyCode: 229}, {key: 'a'}]) {
  assert.equal(key(modifiers), false);
  assert.equal(c.socket().sent.length, 1);
}
assert(key());
assert.equal(c.socket().sent[1].body, 'Line one\nLine two');
assert(key());
assert.equal(c.socket().sent.length, 2, 'pending receipt prevents duplicate sends');
c.result('accepted'); c.socket().close();
assert(key());
assert.equal(c.socket().sent.length, 2, 'disconnected Enter cannot submit');
""")

    def test_enter_uses_comment_validation_and_capabilities(self):
        self.run_js(r"""
const c = client(); c.welcome();
c.elements.get('body').value = '   ';
c.elements.get('body').onkeydown({key: 'Enter', preventDefault() {}});
assert.equal(c.socket().sent.length, 1);
assert.equal(c.validityReports(), 1);
const reactions = client(); reactions.welcome(['reactions', 'comments'], false);
reactions.elements.get('body').value = 'Comment';
reactions.elements.get('body').onkeydown({key: 'Enter', preventDefault() {}});
assert.equal(reactions.socket().sent.length, 1);
""")

    def test_unavailable_state_disables_and_resume_enables_controls(self):
        self.run_js(r"""
const c = client(); c.welcome(['reactions','comments','future'], false);
assert(c.reactions.every(button => button.disabled));
assert(c.elements.get('send-comment').disabled);
c.submit(); assert.equal(c.socket().sent.length, 1);
c.receive({protocol:'qp.audience',version:'1-draft.1',type:'state',revision:'1',available:true});
assert(c.reactions.every(button => !button.disabled));
assert.equal(c.elements.get('send-comment').disabled, false);
""")

    def test_comment_submission_preserves_plain_text_and_original_handle(self):
        self.run_js(r"""
const c = client(); c.welcome();
const body = '<script>alert("x")</script>\n世界';
const handle = ' Name\t ';
c.submit(body, handle);
assert.deepEqual(c.socket().sent[1], {protocol:'qp.audience',version:'1-draft.1', type: 'comment', request_id: '1', body, handle});
assert(c.elements.get('send-comment').disabled);
assert(c.reactions.every(button => button.disabled));
assert.equal(c.elements.get('feedback').textContent, 'Sending…');
""")

    def test_supplementary_unicode_at_both_exact_limits_is_accepted(self):
        self.run_js(r"""
const c = client(); c.welcome();
c.submit('😀'.repeat(200), '🦀'.repeat(32));
assert.equal(c.socket().sent.length, 2);
assert.equal(c.validityReports(), 0);
assert.equal(new TextEncoder().encode(c.socket().sent[1].body).length, 800);
assert.equal(new TextEncoder().encode(c.socket().sent[1].handle).length, 128);
""")

    def test_oversized_or_blank_input_does_not_send_and_reports_validity(self):
        self.run_js(r"""
for (const [body, handle] of [
  ['x'.repeat(201), 'Name'], ['😀'.repeat(201), 'Name'],
  ['Hello', 'x'.repeat(33)], ['Hello', '🦀'.repeat(33)],
  ['', 'Name'], [' \t\r\n\u2003\u00a0', 'Name']
]) {
  const c = client(); c.welcome(); c.submit(body, handle);
  assert.equal(c.socket().sent.length, 1);
  assert.equal(c.validityReports(), 1);
  assert.equal(c.elements.get('send-comment').disabled, false);
}
""")

    def test_correcting_input_clears_custom_validity_and_allows_empty_handle(self):
        self.run_js(r"""
const c = client(); c.welcome();
c.submit('x'.repeat(201), 'x'.repeat(33));
assert(c.elements.get('body').error);
assert(c.elements.get('handle').error);
c.elements.get('body').value = 'Hello';
c.elements.get('handle').value = '';
c.elements.get('body').oninput();
assert.equal(c.elements.get('body').error, '');
assert.equal(c.elements.get('handle').error, '');
c.submit('Hello', '');
assert.equal(c.socket().sent[1].handle, '');
""")

    def test_pending_comment_blocks_reactions_and_duplicate_submissions(self):
        self.run_js(r"""
const c = client(); c.welcome(); c.submit('First');
c.submit('Second'); c.reactions[0].onclick();
assert.equal(c.socket().sent.length, 2);
assert.equal(c.timers.size, 1);
c.result('accepted');
assert.equal(c.elements.get('body').value, 'Second');
assert.equal(c.elements.get('send-comment').disabled, false);
assert.equal(c.timers.size, 0);
c.submit('Second');
assert.equal(c.socket().sent[2].request_id, '2');
""")

    def test_matching_acceptance_clears_only_unchanged_comment_body(self):
        self.run_js(r"""
const c = client(); c.welcome(); c.submit('Hello', 'Name');
c.result('accepted');
assert.equal(c.elements.get('body').value, '');
assert.equal(c.elements.get('handle').value, 'Name');
assert.equal(c.elements.get('feedback').textContent, 'Comment received.');
assert.equal(c.timers.size, 0);
assert(c.reactions.every(button => !button.disabled));
""")

    def test_unmatched_receipt_does_not_finish_pending_comment(self):
        self.run_js(r"""
const c = client(); c.welcome(); c.submit('Hello');
c.result('accepted', 'other');
assert.equal(c.elements.get('body').value, 'Hello');
assert(c.elements.get('send-comment').disabled);
assert.equal(c.timers.size, 1);
assert.equal(c.elements.get('feedback').textContent, 'Sending…');
c.result('accepted');
assert.equal(c.elements.get('body').value, '');
""")

    def test_rejected_receipts_preserve_draft_and_release_pending_state(self):
        self.run_js(r"""
for (const [status, feedback] of [
  ['unknown', 'Delivery could not be confirmed. Messages are not resent automatically.'],
  ['busy', 'The presentation is busy. Try again shortly.'],
  ['stopped', 'This session has ended.']
]) {
  const c = client(); c.welcome(); c.submit('Keep this draft'); c.result(status);
  assert.equal(c.elements.get('body').value, 'Keep this draft');
  assert.equal(c.elements.get('feedback').textContent, feedback);
  assert.equal(c.elements.get('send-comment').disabled, false);
  assert.equal(c.timers.size, 0);
}
""")

    def test_reaction_acceptance_does_not_clear_comment_draft(self):
        self.run_js(r"""
const c = client(); c.welcome();
c.elements.get('body').value = 'Unsubmitted draft';
c.reactions[0].onclick(); c.result('accepted');
assert.equal(c.elements.get('body').value, 'Unsubmitted draft');
assert.equal(c.elements.get('feedback').textContent, 'Reaction received.');
""")

    def test_disconnect_cancels_timeout_disables_inputs_and_preserves_draft(self):
        self.run_js(r"""
const c = client(); c.welcome(); c.submit('Keep this draft'); c.socket().close();
assert.equal(c.timers.size, 0);
assert(c.reactions.every(button => button.disabled));
assert(c.elements.get('send-comment').disabled);
assert.equal(c.elements.get('retry').hidden, false);
assert.equal(c.elements.get('body').value, 'Keep this draft');
c.submit('Still disconnected');
assert.equal(c.socket().sent.length, 2);
""")

    def test_timeout_closes_connection_and_never_replays_comment_on_reconnect(self):
        self.run_js(r"""
const c = client(); c.welcome(); c.submit('Keep this draft');
const timer = [...c.timers.values()][0];
assert.equal(timer.delay, 4000); timer.callback();
assert.equal(c.socket().readyState, 3);
assert.match(c.elements.get('feedback').textContent, /No response/);
c.elements.get('retry').onclick(); c.welcome();
assert.equal(c.socket().sent.length, 1);
assert.equal(c.elements.get('body').value, 'Keep this draft');
c.submit('Keep this draft');
assert.equal(c.socket().sent[1].request_id, '2');
""")

    def test_old_socket_messages_and_close_cannot_mutate_new_pending_comment(self):
        self.run_js(r"""
const c = client(); c.welcome(); c.submit('First');
const old = c.socket(); old.close();
c.elements.get('retry').onclick(); c.welcome(); c.submit('Second');
c.receive({protocol:'qp.audience',version:'1-draft.1', type: 'result', request_id: '2', status: 'accepted'}, old);
old.onclose(); old.onerror();
assert.equal(c.elements.get('body').value, 'Second');
assert(c.elements.get('send-comment').disabled);
assert.equal(c.elements.get('retry').hidden, true);
assert.equal(c.elements.get('status').textContent, 'Connected to the presentation.');
c.result('accepted');
assert.equal(c.elements.get('body').value, '');
""")

    def test_malformed_or_wrong_version_messages_close_and_keep_draft(self):
        self.run_js(r"""
for (const data of ['{', JSON.stringify({v: 2, type: 'welcome', capabilities: ['reactions', 'comments']})]) {
  const c = client(); c.welcome(); c.submit('Draft');
  c.socket().onmessage({data});
  assert.equal(c.socket().readyState, 3);
  assert.equal(c.elements.get('body').value, 'Draft');
  assert(c.elements.get('send-comment').disabled);
  assert.equal(c.timers.size, 0);
}
""")

    def test_strict_receiver_rejects_malformed_frames_without_clearing_pending_draft(self):
        self.run_js(r"""
const welcome = {protocol:'qp.audience',version:'1-draft.1',type:'welcome',connection_id:'a1',capabilities:['comments','reactions'],available:true,limits:{max_message_bytes:4096,request_cache_ms:30000}};
for (const data of [
  JSON.stringify({...welcome, extra:true}),
  JSON.stringify({protocol:'qp.audience',version:'1-draft.1',type:'welcome',available:true}),
  JSON.stringify(welcome).replace('"available":true', '"available":false,"\\u0061vailable":true'),
  JSON.stringify({...welcome, capabilities:['\ud800']}),
  JSON.stringify({...welcome, capabilities:['\udfff']}),
  JSON.stringify({...welcome, capabilities:['x','x']}),
  JSON.stringify({...welcome, limits:{...welcome.limits, extra:true}}),
  JSON.stringify({protocol:'qp.audience',version:'1-draft.1',type:'state',revision:'01',available:true}),
  JSON.stringify({protocol:'qp.audience',version:'1-draft.1',type:'result',request_id:'1',status:'rate_limited'}),
  ' '.repeat(4097), new ArrayBuffer(2)
]) {
  const c=client(); c.welcome(); c.submit('Keep this draft');
  c.socket().onmessage({data});
  assert.equal(c.socket().readyState,3);
  assert(c.elements.get('send-comment').disabled);
  assert.equal(c.elements.get('body').value,'Keep this draft');
  assert.equal(c.timers.size,0);
}
""")

    def test_emoji_sequences_and_limit_preserve_submission(self):
        self.run_js(r"""
for (const body of ['😀 👍 ❤️ 👏 😂 ❓', '👨‍👩‍👧‍👦', '🇯🇵 🇺🇸', '👍🏽 👩🏽‍💻', 'e\u0301 🏳️‍🌈', '😀'.repeat(200)]) {
  const c=client(); c.welcome(['reactions','comments','😀']); c.submit(body,'👩🏽‍💻');
  assert.equal(c.socket().sent[1].body,body);
  assert.equal(c.socket().sent[1].handle,'👩🏽‍💻');
  c.result('accepted'); assert.equal(c.elements.get('body').value,'');
}
""")

    def test_shared_assets_and_layout_integrity(self):
        self.run_js(r"""
const manifest=JSON.parse(fs.readFileSync('assets/audience/SOURCE.json','utf8'));
const {createHash}=require('node:crypto');
for (const [file,hash] of Object.entries(manifest.sha256))
  assert.equal(createHash('sha256').update(fs.readFileSync('assets/audience/'+file)).digest('hex'),hash);
assert.match(html,/value="Anonymous"/);
assert.match(html,/Enter to send · Shift\+Enter for a new line/);
assert.match(html,/<div class="composer">[\s\S]*<textarea[\s\S]*<button id="send-comment"/);
assert.match(script,/Messages are not resent automatically/);
""")


if __name__ == "__main__":
    unittest.main()
