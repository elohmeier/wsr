const high = '€\u0081‚ƒ„…†‡ˆ‰Š‹Œ\u008dŽ\u008f\u0090‘’“”•–—˜™š›œ\u009džŸ';
export function cp1252(text: string) {
	return Uint8Array.from(
		[...text].map((c) => {
			const u = c.codePointAt(0)!,
				k = high.indexOf(c);
			return u < 128 || (u >= 160 && u <= 255) ? u : k >= 0 ? 128 + k : 63;
		}),
	);
}
export const cleanLabel = (text: string) => text.replaceAll('&', '');
export function gridPacket(
	rows: { side: string; strike: string; expiry: string; quantity: string }[],
) {
	const fields = rows.flatMap((row) =>
		[row.side, row.strike, row.expiry, row.quantity].map(cp1252),
	);
	const bytes = new Uint8Array(fields.reduce((n, f) => n + 4 + f.length, 0)),
		view = new DataView(bytes.buffer);
	let p = 0;
	for (const f of fields) {
		view.setUint32(p, f.length, true);
		bytes.set(f, p + 4);
		p += 4 + f.length;
	}
	return bytes;
}
