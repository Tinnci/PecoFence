// UI-thread client for the closed Settings protocol. No DOM and no configuration authority.
(function (root) {
  'use strict';
  const VERSION = 1;
  const paths = {
    language: 'language', theme: 'theme', themeStyle: 'themeStyle', iconSize: 'iconSize',
    autostart: 'autostart', hideRealIcons: 'hideRealIcons', quickHideEnabled: 'quickHide.enabled',
    showDesktop: 'showDesktop', hoverPeek: 'rollUp.hoverPeek', clickToExpand: 'rollUp.clickToExpand',
    titleOnHover: 'rollUp.titleOnHover', hideInactiveScrollbar: 'rollUp.hideInactiveScrollbar',
    snappingEnabled: 'snapping.enabled', peekEnabled: 'peek.enabled', peekDim: 'peek.dim',
    peekHotkey: 'peek.hotkey', iconTint: 'icons.tintRgb', iconTintStrength: 'icons.tintStrength',
    chameleon: 'icons.chameleon',
  };
  const clone = value => structuredClone(value);
  const uuid = () => {
    if (typeof crypto.randomUUID === 'function') return crypto.randomUUID();
    const bytes = crypto.getRandomValues(new Uint8Array(16));
    bytes[6] = (bytes[6] & 15) | 64; bytes[8] = (bytes[8] & 63) | 128;
    const text = [...bytes].map(v => v.toString(16).padStart(2, '0')).join('');
    return `${text.slice(0,8)}-${text.slice(8,12)}-${text.slice(12,16)}-${text.slice(16,20)}-${text.slice(20)}`;
  };
  const same = (a, b) => a && b && a.workspace === b.workspace && a.revision === b.revision;
  const set = (object, path, value) => {
    const keys = path.split('.'); const last = keys.pop();
    for (const key of keys) object = object[key];
    object[last] = clone(value);
  };
  const hex = rgb => rgb ? rgb.map(v => v.toString(16).padStart(2, '0')).join('').toUpperCase() : null;
  function overlay(view, command) {
    if (command.kind === 'setSetting') {
      const path = paths[command.change.property];
      if (path) set(view.settings, path, command.change.value);
    } else if (command.kind === 'setContent' || command.kind === 'setContainer') {
      const content = view.fences.find(f => f.contentId === command.contentId && f.containerId === command.containerId);
      if (!content) return;
      const change = command.change;
      const rows = command.kind === 'setContainer' ? view.fences.filter(f => f.containerId === command.containerId) : [content];
      for (const row of rows) {
        if (change.property === 'portalNavigate') row.portal.navigate = change.value;
        else if (change.property === 'portalTitleIcon') row.portal.titleIcon = change.value;
        else if (change.property === 'tint') row.tint = hex(change.value);
        else if (change.property === 'titleColor') row.titleColor = typeof change.value === 'string' ? change.value : hex(change.value.custom);
        else if (change.property === 'opacity') row.opacity = change.value == null ? 'default' : change.value < 0.8 ? 'clear' : 'solid';
        else if (change.property !== 'dockTop') row[change.property] = clone(change.value);
      }
    } else if (command.kind === 'rule') {
      const change = command.change, list = view.rules.list;
      const index = list.findIndex(r => r.id === change.id);
      if (change.kind === 'create' && index < 0) list.push({ id: change.id, ...clone(change.draft) });
      else if (change.kind === 'edit' && index >= 0) list[index] = { ...list[index], ...clone(change.draft) };
      else if (change.kind === 'setEnabled' && index >= 0) list[index].enabled = change.value;
      else if (change.kind === 'delete' && index >= 0) list.splice(index, 1);
      else if (change.kind === 'keepUpdated') view.rules.keepUpdated = change.value;
      else if (change.kind === 'defaultTarget') view.rules.defaultTarget = clone(change.target);
      else if (change.kind === 'move' && index >= 0 && change.before !== change.id) {
        const row = list.splice(index, 1)[0];
        const before = change.before == null ? list.length : list.findIndex(r => r.id === change.before);
        if (before < 0) list.splice(index, 0, row); else list.splice(before, 0, row);
      }
    }
  }
  class Client {
    constructor(options) {
      this.options = options;
      this.page = uuid(); this.client = null; this.stamp = null;
      this.sequence = 0; this.viewSequence = -1; this.view = null;
      this.queue = []; this.inFlight = null; this.failed = []; this.accepted = [];
      this.persistence = null;
      this.healthy = true;
    }
    start() { this.options.send({ type: 'ready', protocol: VERSION, page: this.page }); }
    submit(command, base = this.stamp) {
      if (!this.healthy || !this.client || !this.stamp || this.queue.length + this.failed.length + this.accepted.length + Number(!!this.inFlight) >= 64) {
        this.options.onError?.('protocol', 'Settings is not connected or the queue is full'); return false;
      }
      this.queue.push({ command: clone(command), base: clone(base) });
      this.emit(); this.pump(); return true;
    }
    pump() {
      if (!this.healthy || this.inFlight || !this.queue.length) return;
      const entry = this.queue.shift();
      this.inFlight = {
        ...entry, request: { type: 'request', protocol: VERSION, client: this.client,
          sequence: ++this.sequence, base: clone(entry.base), command: clone(entry.command) },
      };
      this.options.send(clone(this.inFlight.request));
    }
    retryInFlight() {
      if (this.healthy && this.inFlight) this.options.send(clone(this.inFlight.request));
    }
    retryDrafts() {
      const drafts = this.failed.splice(0);
      for (const entry of drafts) {
        if (entry.base.workspace === this.stamp?.workspace) this.queue.push({ command: entry.command, base: clone(this.stamp) });
      }
      this.emit(); this.pump();
    }
    discardDrafts() { this.failed = []; this.emit(); }
    receive(message) {
      if (message.type === 'protocolError') {
        this.healthy = false;
        this.options.onError?.('protocol', message.detail);
        this.emit();
        return true;
      }
      if (message.type === 'snapshot') {
        if (message.protocol !== VERSION || message.page !== this.page || message.sequence <= this.viewSequence) return true;
        if (this.client && this.client !== message.client) return true;
        const changedWorkspace = this.stamp && this.stamp.workspace !== message.stamp.workspace;
        this.client = message.client;
        this.stamp = clone(message.stamp); this.viewSequence = message.sequence;
        this.view = clone(message.view);
        if (changedWorkspace) {
          const discarded = this.queue.length + this.failed.length;
          this.queue = []; this.failed = []; this.accepted = [];
          this.persistence = null;
          if (discarded) this.options.onError?.('workspace', '');
        }
        this.accepted = this.accepted.filter(entry => entry.current.workspace === this.stamp.workspace && entry.current.revision > this.stamp.revision);
        this.emit(); this.pump(); return true;
      }
      if (message.type === 'receipt') {
        const entry = this.inFlight;
        if (!entry || message.client !== this.client || message.sequence !== entry.request.sequence || !same(message.base, entry.request.base)) return true;
        this.inFlight = null;
        if (message.rejected) {
          if (entry.base.workspace === this.stamp?.workspace) this.failed.push(entry);
          const rejection = typeof message.rejected === 'string' ? message.rejected : Object.keys(message.rejected)[0];
          const detail = typeof message.rejected === 'string' ? '' : message.rejected[rejection];
          this.options.onError?.(rejection, detail);
          if (['client', 'sequence', 'expired', 'reusedSequence', 'protocol'].includes(rejection)) this.healthy = false;
        } else if (!message.cancelled) {
          this.accepted.push({ ...entry, current: clone(message.current) });
          // Rebase only over our own acknowledged edit, never over a newer external snapshot.
          for (const next of this.queue) if (same(next.base, message.base)) next.base = clone(message.current);
          this.options.onAccepted?.(message, entry.command);
        }
        this.options.onDecision?.(message, entry.command);
        this.emit(); this.pump(); return true;
      }
      if (message.type === 'persistence') {
        if (message.page === this.page && message.client === this.client && message.stamp.workspace === this.stamp?.workspace) {
          // Never let a delayed older save acknowledgement hide a newer pending edit.
          const previous = this.persistence;
          if (message.stamp.revision < this.stamp.revision
              || (previous && (message.stamp.revision < previous.stamp.revision
                || (message.committedRevision ?? -1) < (previous.committedRevision ?? -1)))) return true;
          this.persistence = clone(message);
          this.options.onPersistence?.(message);
        }
        return true;
      }
      return false;
    }
    emit() {
      if (!this.view) return;
      const view = clone(this.view);
      const entries = [...this.accepted, ...this.failed, ...(this.inFlight ? [this.inFlight] : []), ...this.queue];
      for (const entry of entries) if (entry.base.workspace === this.stamp.workspace) overlay(view, entry.command);
      this.options.onSnapshot?.(view, this.stamp, this.failed.length);
    }
  }
  const api = { Client, paths, overlay, VERSION, uuid };
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.PecoFenceSettings = api;
})(typeof window === 'object' ? window : globalThis);
