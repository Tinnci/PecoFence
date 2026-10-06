'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const { Client } = require('../ui/settings-client.js');

const stamp = revision => ({ workspace: 'workspace-a', revision });
const setting = (property, value) => ({ kind: 'setSetting', change: { property, value } });
function harness() {
  const sent = [], errors = [], views = [], persistence = [];
  const client = new Client({
    send: m => sent.push(m), onError: (...e) => errors.push(e),
    onSnapshot: (view, s, drafts) => views.push({ view, stamp: s, drafts }),
    onPersistence: m => persistence.push(m),
  });
  const view = {
    settings: { autostart: false, peek: { enabled: true }, icons: { tintRgb: null, tintStrength: 0.8 } },
    fences: [], rules: { list: [], keepUpdated: false, defaultTarget: 'inbox' },
  };
  let sequence = 0;
  function snapshot(s = stamp(0), v = view, page = client.page) {
    client.receive({ type: 'snapshot', protocol: 1, page, client: 'client-a', stamp: s, sequence: ++sequence, view: v });
  }
  function receipt(current, rejected = null, cancelled = false) {
    const r = client.inFlight.request;
    client.receive({ type: 'receipt', client: r.client, sequence: r.sequence, base: r.base, current, rejected, cancelled });
  }
  function persisted(s, committedRevision, issue = null) {
    client.receive({ type: 'persistence', page: client.page, client: 'client-a', stamp: s, committedRevision, issue });
  }
  client.start(); snapshot();
  return { client, sent, errors, views, persistence, view, snapshot, receipt, persisted };
}

test('FIFO edits rebase only over our own accepted revision', () => {
  const h = harness();
  h.client.submit(setting('autostart', true));
  h.client.submit(setting('peekEnabled', false));
  assert.equal(h.sent.length, 2); // ready + first request
  h.receipt(stamp(1));
  assert.deepEqual(h.client.inFlight.request.base, stamp(1));
  assert.equal(h.client.inFlight.request.sequence, 2);
  assert.equal(h.views.at(-1).view.settings.autostart, true);
  assert.equal(h.views.at(-1).view.settings.peek.enabled, false);
  assert.equal(h.view.settings.autostart, false); // authoritative snapshot was not mutated
});

test('external snapshots do not silently rebase a queued draft', () => {
  const h = harness();
  h.client.submit(setting('autostart', true));
  h.client.submit(setting('peekEnabled', false));
  h.snapshot(stamp(3));
  h.receipt(stamp(3), 'conflict');
  assert.deepEqual(h.client.inFlight.request.base, stamp(0));
  h.receipt(stamp(3), 'conflict');
  assert.equal(h.client.failed.length, 2);
  assert.equal(h.views.at(-1).view.settings.autostart, true);
  h.client.retryDrafts(); // explicit user choice, not automatic conflict overwrite
  assert.deepEqual(h.client.inFlight.request.base, stamp(3));
  assert.equal(h.client.inFlight.request.sequence, 3);
});

test('a form can submit against the revision captured when editing began', () => {
  const h = harness(), base = structuredClone(h.client.stamp);
  h.snapshot(stamp(4));
  h.client.submit(setting('autostart', true), base);
  assert.deepEqual(h.client.inFlight.request.base, stamp(0));
});

test('retrying an in-flight request resends exactly the same identity and payload', () => {
  const h = harness(), command = setting('autostart', true);
  h.client.submit(command);
  command.change.value = false;
  const first = structuredClone(h.sent.at(-1));
  h.client.retryInFlight();
  assert.deepEqual(h.sent.at(-1), first);
});

test('a repeated receipt cannot settle the next operation', () => {
  const h = harness();
  h.client.submit(setting('autostart', true)); h.client.submit(setting('peekEnabled', false));
  const request = h.client.inFlight.request;
  h.receipt(stamp(1));
  h.client.receive({ type: 'receipt', client: request.client, sequence: request.sequence, base: request.base, current: stamp(1) });
  assert.equal(h.client.inFlight.request.sequence, 2);
});

test('rejected values remain drafts until explicit discard', () => {
  const h = harness();
  h.client.submit(setting('autostart', true)); h.receipt(stamp(0), { invalid: 'bad value' });
  assert.equal(h.views.at(-1).drafts, 1);
  assert.equal(h.views.at(-1).view.settings.autostart, true);
  h.client.discardDrafts();
  assert.equal(h.views.at(-1).view.settings.autostart, false);
});

test('workspace replacement drops queued drafts and never overlays old values', () => {
  const h = harness();
  h.client.submit(setting('autostart', true)); h.client.submit(setting('peekEnabled', false));
  h.snapshot({ workspace: 'workspace-b', revision: 0 });
  assert.equal(h.client.queue.length, 0);
  assert.equal(h.views.at(-1).view.settings.autostart, false);
  h.receipt({ workspace: 'workspace-b', revision: 0 }, 'workspace');
  assert.equal(h.client.failed.length, 0);
});

test('old page, old client, old snapshot order and mismatched receipts are ignored', () => {
  const h = harness();
  h.snapshot(stamp(8), h.view, 'old-page');
  assert.deepEqual(h.client.stamp, stamp(0));
  h.client.receive({ type: 'snapshot', protocol: 1, page: h.client.page, client: 'wrong-client', stamp: stamp(9), sequence: 50, view: h.view });
  h.client.submit(setting('autostart', true));
  h.client.receive({ type: 'receipt', client: 'wrong-client', sequence: 1, base: stamp(0), current: stamp(1) });
  assert.ok(h.client.inFlight);
  h.snapshot(stamp(2));
  h.client.receive({ type: 'snapshot', protocol: 1, page: h.client.page, client: 'client-a', stamp: stamp(0), sequence: 0, view: h.view });
  assert.deepEqual(h.client.stamp, stamp(2));
});

test('accepted is not a disk commit, and delayed persistence cannot hide new edits', () => {
  const h = harness();
  h.client.submit(setting('autostart', true)); h.receipt(stamp(1));
  assert.equal(h.persistence.length, 0);
  h.snapshot(stamp(1));
  h.persisted(stamp(0), 0);
  assert.equal(h.persistence.length, 0);
  h.persisted(stamp(1), 0);
  h.persisted(stamp(1), 1);
  h.persisted(stamp(1), 0);
  assert.equal(h.persistence.length, 2);
  assert.equal(h.persistence.at(-1).committedRevision, 1);
});

test('backup degradation can still report a committed primary revision', () => {
  const h = harness();
  h.snapshot(stamp(2));
  h.persisted(stamp(2), 2, 'backup failed');
  assert.equal(h.persistence.at(-1).committedRevision, 2);
  assert.equal(h.persistence.at(-1).issue, 'backup failed');
});

test('container drafts project to every tab, not unrelated containers', () => {
  const h = harness();
  h.view.fences = [{ contentId: 'a', containerId: 'c' }, { contentId: 'b', containerId: 'c' }, { contentId: 'z', containerId: 'd' }];
  h.snapshot();
  h.client.submit({ kind: 'setContainer', contentId: 'a', containerId: 'c', change: { property: 'locked', value: true } });
  assert.deepEqual(h.views.at(-1).view.fences.map(f => f.locked), [true, true, undefined]);
});

test('rule edits overlay one rule and preserve identity/template metadata', () => {
  const h = harness();
  h.view.rules.list = [{ id: 'a', name: 'original', template: 'music' }, { id: 'b', name: 'untouched' }];
  h.snapshot();
  h.client.submit({ kind: 'rule', change: { kind: 'edit', id: 'a', draft: { name: 'edited', allOf: [{ cond: 'origin', value: 'publicDesktop' }] } } });
  const list = h.views.at(-1).view.rules.list;
  assert.equal(list[0].template, 'music'); assert.equal(list[1].name, 'untouched');
});

test('protocol failure disables edits and republishes the disabled view', () => {
  const h = harness(), count = h.views.length;
  h.client.receive({ type: 'protocolError', detail: 'wrong version' });
  assert.equal(h.client.healthy, false);
  assert.equal(h.views.length, count + 1);
  assert.equal(h.client.submit(setting('autostart', true)), false);
});

test('unanswered edits cannot grow an unbounded queue', () => {
  const h = harness();
  for (let i = 0; i < 64; i++) assert.equal(h.client.submit(setting('autostart', !!(i % 2))), true);
  assert.equal(h.client.submit(setting('autostart', true)), false);
  assert.equal(h.client.queue.length, 63);
});
