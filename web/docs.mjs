const urls = new Map();
const assets = new Map(), decoder = new TextDecoder('windows-1252');
export function cacheAssets(files) {
  for (const file of files || []) {
    if (/\.(TXT|HTM|JPG|ORI|BMP)$/.test(file.name) || file.name === 'CORPNAME.DAT') {
      if(urls.has(file.name)){URL.revokeObjectURL(urls.get(file.name));urls.delete(file.name);}
      if (file.bytes) assets.set(file.name,file.bytes); else assets.delete(file.name);
    }
  }
}
export function assetUrl(name) {
  const bytes=assets.get(name); if(!bytes) return undefined;
  if(!urls.has(name)) urls.set(name,URL.createObjectURL(new Blob([bytes],{type:'image/bmp'})));
  return urls.get(name);
}
export const asset = name => assets.get(name);
export function helpTopics(bytes) {
  const source = decoder.decode(bytes || new Uint8Array()), matches = [...source.matchAll(/^@I(\d+)\s*\r?$/gm)];
  return matches.map((match,i) => {
    const text = source.slice(match.index+match[0].length,matches[i+1]?.index ?? source.length).trim();
    return {id:match[1],title:text.split(/\r?\n/)[0].replace(/^\s*-\s*/,''),text};
  });
}
export function manualBody(bytes) {
  const parsed = new DOMParser().parseFromString(decoder.decode(bytes),'text/html'), out = document.createElement('div'), urls = [];
  const allowed = new Set('A P DIV SPAN TABLE THEAD TBODY TR TH TD B STRONG I EM U H1 H2 H3 H4 H5 H6 HR BR UL OL LI DL DT DD BLOCKQUOTE PRE FONT IMG CENTER SUP SUB'.split(' '));
  function copy(node, parent) {
    if (node.nodeType === 3) { parent.append(node.textContent); return; }
    if (node.nodeType !== 1 || ['SCRIPT','STYLE','IFRAME','OBJECT','FORM'].includes(node.tagName)) return;
    const el = allowed.has(node.tagName) ? document.createElement(node.tagName) : document.createElement('span');
    const anchor = node.getAttribute('id') || node.getAttribute('name');
    if (anchor) el.id = 'doc-'+anchor;
    if (node.tagName === 'A') {
      const href = node.getAttribute('href') || '';
      if (href.startsWith('#')) {
        el.href = '#doc-'+href.slice(1);
        el.onclick = e => { e.preventDefault(); out.querySelector('[id="'+CSS.escape('doc-'+href.slice(1))+'"]')?.scrollIntoView(); };
      } else if (/^https?:\/\//i.test(href)) {
        el.href = href; el.target = '_blank'; el.rel = 'noopener noreferrer';
      }
    }
    if (node.tagName === 'IMG') {
      const name = (node.getAttribute('src') || '').split(/[\\/]/).pop().toUpperCase(), image = assets.get(name);
      el.alt = node.getAttribute('alt') || '';
      if (image) { const url = URL.createObjectURL(new Blob([image],{type:'image/jpeg'})); urls.push(url); el.src = url; }
    }
    for (const attr of ['colspan','rowspan']) {
      const n = +node.getAttribute(attr); if (n > 0 && n < 100) el.setAttribute(attr,n);
    }
    for (const child of node.childNodes) copy(child,el);
    parent.append(el);
  }
  for (const child of parsed.body.childNodes) copy(child,out);
  return {out,dispose:() => urls.forEach(url => URL.revokeObjectURL(url))};
}
