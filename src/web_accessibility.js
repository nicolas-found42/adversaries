// A DOM adapter for egui's own AccessKit nodes. No copied product state or API calls.
const actions = [];
let active = false;
let latest = null;
let latestJson = '';
let latestWorkspace = '';
let currentWorkspace = '';
const workspaceScroll = new Map();
let lastFocus = '';
let dialogId = '';
let returnScroll = 0;
let returnElement = null;
let popupOpener = null;
let wake = () => {};
export function setWake(callback) { wake = callback; }
const cache = new Map();
const panel = document.getElementById('html_controls');
const content = document.getElementById('html_control_tree');
const toggle = document.getElementById('html_controls_toggle');
const skip = document.getElementById('skip_to_workspace');
const canvas = document.getElementById('the_canvas_id');
// Reserve actual switch height outside the canvas, including enlarged labels.
const switchObserver = new ResizeObserver(() => {
  const height = Math.ceil(toggle.getBoundingClientRect().height) + 16;
  document.documentElement.style.setProperty('--view-switch-space', `${height}px`);
});
switchObserver.observe(toggle);

function request(node, action, value) {
  actions.push({id: node.id, action, value: value ?? null});
  wake();
}

function setActive(value) {
  if (active && currentWorkspace && !dialogId) workspaceScroll.set(currentWorkspace, window.scrollY);
  active = value;
  document.documentElement.classList.toggle('reading-view', active);
  panel.hidden = !active;
  canvas.style.visibility = active ? 'hidden' : '';
  canvas.setAttribute('aria-hidden', String(active));
  toggle.textContent = active ? 'Use visual workspace' : 'Use reading view';
  toggle.setAttribute('aria-expanded', String(active));
  if (active && latest) {
    renderWorkspace(latest, latestWorkspace);
    window.scrollTo(0, dialogId ? 0 : (workspaceScroll.get(currentWorkspace) || 0));
  }
  wake();
}

toggle.addEventListener('click', () => setActive(!active));

export function isActive() { return active; }
export function takeActions() { return JSON.stringify(actions.splice(0)); }
document.addEventListener('keydown', event => {
  if (!active || event.key !== 'Escape') return;
  event.preventDefault();
  actions.push({id: '0', action: 'escape', value: null});
  wake();
});

function isInput(node) {
  return node.set_value && node.numeric === null;
}

function isButton(node) {
  const staticRoles = ['Label', 'Paragraph', 'Heading', 'TextRun', 'ScrollBar', 'Status'];
  return node.click && Boolean(node.label || node.value) && !staticRoles.includes(node.role);
}

function isContainer(node) {
  return ['Navigation', 'Main', 'ContentInfo', 'Group'].includes(node.role);
}

function structureTag(node) {
  if (node.document) return 'article';
  if (node.role === 'Heading') return `h${Math.min(6, Math.max(1, node.level || 2))}`;
  return {Navigation:'nav', Main:'main', ContentInfo:'footer', Label:'p', Paragraph:'p'}[node.role] || 'div';
}

function tag(node) {
  if (isInput(node)) return node.role === 'MultilineTextInput' ? 'textarea' : 'input';
  if (node.numeric !== null && node.set_value) return 'input';
  if (isButton(node)) return 'button';
  return structureTag(node);
}

function accessibleName(node) {
  return node.label || node.placeholder || (node.numeric !== null ? 'Value' : 'Text input');
}

function makeControl(node, kind) {
  const control = document.createElement(kind);
  if (kind === 'button') control.type = 'button';
  if (!['button', 'input', 'textarea'].includes(kind)) return control;
  control.addEventListener('focus', () => request(control.councilNode, 'focus'));
  control.addEventListener('click', () => {
    if (control.councilNode.role === 'ComboBox') popupOpener = control;
    if (control.tagName === 'BUTTON' && control.councilNode.click) request(control.councilNode, 'click');
  });
  if (kind === 'input' || kind === 'textarea') {
    control.addEventListener('input', () => {
      const current = control.councilNode;
      request(current, current.numeric === null ? 'value' : 'numeric', control.value);
    });
  }
  return control;
}

function setOptional(element, key, value) {
  if (value === null || value === undefined) element.removeAttribute(key);
  else if (element.getAttribute(key) !== String(value)) element.setAttribute(key, String(value));
}

function updateInput(control, node) {
  const value = node.numeric === null ? node.value : String(node.numeric);
  if (control.value !== value) control.value = value;
  control.disabled = node.disabled;
  control.readOnly = node.readonly;
  control.placeholder = node.placeholder;
  control.setAttribute('aria-label', accessibleName(node));
  if (control.tagName !== 'INPUT') return;
  control.type = node.numeric === null ? 'text' : 'number';
  setOptional(control, 'min', node.min);
  setOptional(control, 'max', node.max);
  setOptional(control, 'step', node.step);
}

function updateControl(control, node) {
  control.councilNode = node;
  control.id = `semantic-${node.id}`;
  setOptional(control, 'aria-live', node.live ? 'polite' : null);
  setOptional(control, 'role', node.role === 'Status' ? 'status' : null);
  const kind = control.tagName;
  if (isContainer(node)) updateContainer(control, node);
  else if (node.document) updateDocument(control, node.document);
  else if (kind === 'INPUT' || kind === 'TEXTAREA') updateInput(control, node);
  else {
    const text = node.label || node.value;
    if (control.textContent !== text) control.textContent = text;
  }
  updateButton(control, node);
}

function updateButton(control, node) {
  if (control.tagName !== 'BUTTON') return;
  control.disabled = node.disabled;
  setOptional(control, 'aria-expanded', node.expanded);
  setOptional(control, 'aria-pressed', node.toggled);
  if (!control.textContent) control.textContent = accessibleName(node);
}

function updateContainer(control, node) {
  setOptional(control, 'aria-label', node.label || null);
  const roles = {Group:'group', Main:'main', Navigation:'navigation', ContentInfo:'contentinfo'};
  setOptional(control, 'role', roles[node.role] || null);
  control.className = node.horizontal ? 'semantic-container semantic-flow' : 'semantic-container';
  if (node.role === 'Main') control.tabIndex = -1;
}

function entry(node) {
  const kind = tag(node), container = isContainer(node);
  let item = cache.get(node.id);
  if (!item || item.kind !== kind || item.container !== container) {
    if (item) documentObserver.unobserve(item.control);
    const control = makeControl(node, kind);
    item = {kind, container, wrapper:document.createElement('div'), control, children:container ? control : document.createElement('div')};
    item.wrapper.className = 'semantic-node';
    item.wrapper.append(item.control);
    if (!container) { item.children.className = 'semantic-children'; item.wrapper.append(item.children); }
    cache.set(node.id, item);
  }
  updateControl(item.control, node);
  return item;
}

function populate(parent, nodes, live) {
  const desired = nodes.map(node => {
    live.add(node.id);
    const item = entry(node);
    const children = item.kind === 'button' ? node.children.filter(child => child.role !== 'Label') : node.children;
    populate(item.children, children, live);
    return item.wrapper;
  });
  reconcile(parent, desired);
}

function reconcile(parent, desired) {
  desired.forEach((element, index) => {
    if (parent.children[index] !== element) parent.insertBefore(element, parent.children[index] || null);
  });
  while (parent.children.length > desired.length) parent.lastElementChild.remove();
}

function findDialog(node) {
  if (node.role === 'Dialog' || node.role === 'AlertDialog') return node;
  for (const child of node.children) {
    const result = findDialog(child);
    if (result) return result;
  }
  return null;
}

function updateDialog(dialog) {
  const id = dialog?.id || '';
  if (id === dialogId) return;
  if (id) {
    window.scrollTo(0, 0);
    content.querySelector('textarea, input, button')?.focus({preventScroll: true});
  } else {
    window.scrollTo(0, returnScroll);
    const opener = returnElement?.isConnected ? returnElement : document.getElementById(returnElement?.id || '');
    opener?.focus({preventScroll: true});
  }
  dialogId = id;
}

function hasListBox(node) {
  return node.role === 'ListBox' || node.children.some(hasListBox);
}

function restorePopupFocus(tree) {
  if (!popupOpener || hasListBox(tree)) return;
  if (document.activeElement === document.body && popupOpener.isConnected) popupOpener.focus({preventScroll: true});
  popupOpener = null;
}

function captureDialogReturn(dialog) {
  // Capture before reconciliation removes the focused background control.
  if (dialogId || !dialog) return;
  returnScroll = window.scrollY;
  returnElement = document.activeElement;
}

function render(tree) {
  const live = new Set();
  const dialog = findDialog(tree);
  captureDialogReturn(dialog);
  populate(content, dialog ? [dialog] : tree.children, live);
  for (const [key, item] of cache) {
    if (live.has(key)) continue;
    documentObserver.unobserve(item.control);
    cache.delete(key);
  }
  content.setAttribute('aria-label', dialog ? 'Current dialog' : 'Council workspace');
  content.setAttribute('role', dialog ? 'dialog' : 'region');
  setOptional(content, 'aria-modal', dialog ? true : null);
  setOptional(content, 'aria-labelledby', dialog ? content.querySelector('h2')?.id : null);
  const main = content.querySelector('main');
  skip.hidden = !main || Boolean(dialog);
  if (main) skip.href = `#${main.id}`;
  updateDialog(dialog);
  restorePopupFocus(tree);
}

function renderWorkspace(tree, workspace) {
  if (workspace === currentWorkspace) { render(tree); return; }
  if (currentWorkspace && !dialogId) workspaceScroll.set(currentWorkspace, window.scrollY);
  render(tree);
  window.scrollTo(0, workspaceScroll.get(workspace) || 0);
  currentWorkspace = workspace;
}

export function syncTree(json, focused, workspace, scale) {
  const fontSize = `${1.0625 * scale}rem`;
  if (panel.style.fontSize !== fontSize) panel.style.fontSize = fontSize;
  const changed = json !== latestJson;
  if (changed) { latest = JSON.parse(json); latestJson = json; }
  latestWorkspace = workspace;
  if (!active) return;
  const changedWorkspace = workspace !== currentWorkspace;
  if (changed || changedWorkspace) renderWorkspace(latest, workspace);
  followNativeFocus(focused, changedWorkspace, workspace);
}

function followNativeFocus(focused, changedWorkspace, workspace) {
  const ownsFocus = content.contains(document.activeElement) || document.activeElement === document.body;
  if (focused && (focused !== lastFocus || changedWorkspace) && ownsFocus) {
    const preventScroll = !changedWorkspace || workspaceScroll.has(workspace);
    cache.get(focused)?.control.focus({preventScroll});
  }
  lastFocus = focused;
}


// Read-only Markdown structure comes from the shared Rust reader. These nodes
// have no product actions and never interpret generated text as HTML or script.
const documentTags = {
  paragraph: 'p', quote: 'blockquote', item: 'li', emphasis: 'em', strong: 'strong',
  strike: 'del', code: 'code', code_block: 'pre', break: 'br', rule: 'hr',
  superscript: 'sup', subscript: 'sub', footnote: 'aside', footnote_reference: 'sup',
  definition_list: 'dl', definition_title: 'dt', definition: 'dd', literal_block: 'pre',
  link: 'a', image: 'span', task: 'span', span: 'span',
};
const documentObserver = new ResizeObserver(entries => {
  for (const entry of entries) {
    const small = entry.contentRect.width < 44 * parseFloat(getComputedStyle(entry.target).fontSize);
    entry.target.classList.toggle('reader-stacked', small);
  }
});

function documentText(node) {
  return (node.text || '') + (node.children || []).map(documentText).join('');
}

function documentAnchor(scope, fragment) {
  return `${scope}-md-${encodeURIComponent(fragment)}`;
}

function safeDocumentHref(href, scope) {
  if (href.startsWith('#')) {
    try { return `#${documentAnchor(scope, decodeURIComponent(href.slice(1)))}`; }
    catch { return null; }
  }
  // Relative paths have no reliable base in a saved provider response.
  if (!/^(https?:|mailto:)/i.test(href)) return null;
  try {
    const url = new URL(href);
    return ['http:', 'https:', 'mailto:'].includes(url.protocol) ? url.href : null;
  } catch { return null; }
}

function configureDocumentLink(element, node, scope) {
  const href = safeDocumentHref(node.href || '', scope);
  if (href) element.href = href;
  else element.removeAttribute('href');
  if (href && !href.startsWith('#')) {
    element.target = '_blank';
    element.rel = 'noopener noreferrer';
  }
  if (node.title) element.title = node.title;
}

function documentFootnoteReference(node, scope) {
  const element = document.createElement('sup');
  const link = document.createElement('a');
  configureDocumentLink(link, node, scope);
  link.textContent = node.text;
  link.setAttribute('aria-label', `Footnote ${node.text}`);
  element.append(link);
  return element;
}

function documentImage(node, scope) {
  const element = document.createElement('span');
  element.className = 'reader-image';
  element.append(document.createTextNode(`Image: ${documentText(node) || node.title || 'No description'}`));
  const href = safeDocumentHref(node.href || '', scope);
  if (href && /^https?:/i.test(href)) {
    const link = document.createElement('a');
    configureDocumentLink(link, node, scope);
    link.textContent = 'Open image';
    element.append(document.createTextNode(' · '), link);
  }
  return element;
}

function documentTask(node) {
  const element = document.createElement('span');
  element.className = 'reader-task';
  element.setAttribute('role', 'img');
  element.setAttribute('aria-label', node.checked ? 'Completed task' : 'Incomplete task');
  element.textContent = node.checked ? '☑ ' : '☐ ';
  return element;
}

function documentTag(node) {
  if (node.kind === 'heading') return `h${Math.min(6, Math.max(1, node.level || 2))}`;
  if (node.kind === 'list') return node.start == null ? 'ul' : 'ol';
  return documentTags[node.kind] || 'span';
}

function configureDocumentNode(element, node, scope) {
  if (node.anchor) element.id = documentAnchor(scope, node.anchor);
  if (node.kind === 'list' && node.start != null) element.start = node.start;
  if (node.kind === 'link') configureDocumentLink(element, node, scope);
  if (node.kind === 'footnote') element.setAttribute('aria-label', `Footnote ${node.title}`);
}

function ordinaryDocumentNode(node, scope) {
  const element = document.createElement(documentTag(node));
  configureDocumentNode(element, node, scope);
  const children = (node.children || []).map(child => documentNode(child, scope));
  if (node.text) element.append(document.createTextNode(node.text));
  if (node.kind === 'code_block') {
    const code = document.createElement('code');
    code.append(...children);
    element.append(code);
  } else element.append(...children);
  return element;
}

function documentCells(nodes, headers, scope, align, heading) {
  const row = document.createElement('tr');
  row.setAttribute('role', 'row');
  nodes.forEach((node, index) => {
    const cell = document.createElement(heading ? 'th' : 'td');
    cell.setAttribute('role', heading ? 'columnheader' : 'cell');
    if (heading) cell.scope = 'col';
    cell.style.textAlign = align[index] || 'start';
    if (!heading && headers[index]) {
      const label = document.createElement('span');
      label.className = 'reader-cell-label';
      label.setAttribute('aria-hidden', 'true');
      label.textContent = headers[index];
      cell.append(label);
    }
    cell.append(...(node.children || []).map(child => documentNode(child, scope)));
    row.append(cell);
  });
  return row;
}

function documentTable(node, scope) {
  const table = document.createElement('table');
  table.setAttribute('role', 'table');
  const head = (node.children || []).find(child => child.kind === 'table_head');
  const cells = head?.children || [];
  const headers = cells.map(documentText);
  const align = node.align || [];
  const thead = document.createElement('thead');
  thead.setAttribute('role', 'rowgroup');
  thead.append(documentCells(cells, headers, scope, align, true));
  const tbody = document.createElement('tbody');
  tbody.setAttribute('role', 'rowgroup');
  const rows = (node.children || []).filter(child => child.kind === 'row');
  for (const row of rows) tbody.append(documentCells(row.children || [], headers, scope, align, false));
  table.append(thead, tbody);
  return table;
}

function documentNode(node, scope) {
  if (node.kind === 'text') return document.createTextNode(node.text || '');
  if (node.kind === 'table') return documentTable(node, scope);
  if (node.kind === 'footnote_reference') return documentFootnoteReference(node, scope);
  if (node.kind === 'image') return documentImage(node, scope);
  if (node.kind === 'task') return documentTask(node);
  return ordinaryDocumentNode(node, scope);
}

function updateDocument(control, value) {
  const json = JSON.stringify(value);
  if (control.councilDocument === json) return;
  control.councilDocument = json;
  control.className = 'reader-document';
  control.replaceChildren(...value.blocks.map(node => documentNode(node, control.id)));
  documentObserver.observe(control);
}
