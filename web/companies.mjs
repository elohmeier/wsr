// CORPNAME is a template for future games, independent of the current game.
const decoder = new TextDecoder('windows-1252');
const high = '€\u0081‚ƒ„…†‡ˆ‰Š‹Œ\u008dŽ\u008f\u0090‘’“”•–—˜™š›œ\u009džŸ';
function encode(text) {
  return Uint8Array.from([...text].map(c => {
    const u = c.codePointAt(0), k = high.indexOf(c);
    if (u < 128 || u >= 160 && u <= 255) return u;
    if (k >= 0) return 128 + k;
    throw new Error('Company names must use Windows Western European characters.');
  }));
}
export function parseCompanies(bytes) {
  const rows = decoder.decode(bytes).split(/\r?\n/).filter(s => /^\d+\|/.test(s)).map(line => {
    const [id, country, symbol, name] = line.split('|');
    return {id:+id, country:+country, symbol:symbol?.trim(), name:name?.trim()};
  });
  if (rows.length !== 1590 || rows.some((r,i) => r.id !== i+11 || !Number.isInteger(r.country) || r.country < 0 || r.country > 53 ||
      !/^[A-Z]{1,5}$/.test(r.symbol) || !r.name || r.name.length > 26 || /[|"\r\n]/.test(r.name))) {
    throw new Error('CORPNAME.DAT does not contain the complete company list.');
  }
  for (const field of ['symbol','name']) {
    if (new Set(rows.map(r => r[field].toUpperCase())).size !== rows.length) throw new Error('Company '+field+' must be unique.');
  }
  return rows;
}
export function editCompany(rows, id, edit) {
  const name = edit.name.trim(), symbol = edit.symbol.trim().toUpperCase(), country = +edit.country;
  if (!name || name.length > 25 || /[|"\r\n]/.test(name)) throw new Error('Use a company name of 1–25 characters, without quotation marks or |.');
  encode(name);
  if (!/^[A-Z]{1,5}$/.test(symbol) || symbol === 'FDIC') throw new Error('Use 1–5 letters for the symbol. FDIC is reserved.');
  if (!Number.isInteger(country) || country < 0 || country > 53) throw new Error('Choose a home country.');
  if (rows.some(r => r.id !== id && r.name.toUpperCase() === name.toUpperCase())) throw new Error('Another company uses this name; used.');
  if (rows.some(r => r.id !== id && r.symbol === symbol)) throw new Error('Another company uses this symbol; used.');
  if (!rows.some(r => r.id === id)) throw new Error('Choose a company.');
  return rows.map(r => r.id === id ? {id, name, symbol, country} : r);
}
export function serializeCompanies(rows) {
  const bytes = encode(rows.map(r => String(r.id).padStart(4,'0')+'|'+String(r.country).padStart(2,'0')+'|'+r.symbol.padEnd(5)+'|'+r.name+'|').join('\r\n')+'\r\n@END|\r\n');
  parseCompanies(bytes);
  return bytes;
}
