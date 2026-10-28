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
export function fieldsPacket(values: string[]) {
	const fields = values.map((value) =>
		cp1252(value == null ? '' : String(value)),
	);
	const bytes = new Uint8Array(fields.reduce((n, f) => n + 4 + f.length, 0));
	const view = new DataView(bytes.buffer);
	let p = 0;
	for (const field of fields) {
		view.setUint32(p, field.length, true);
		bytes.set(field, p + 4);
		p += 4 + field.length;
	}
	return bytes;
}
export function gridPacket(
	rows: { side: string; strike: string; expiry: string; quantity: string }[],
) {
	return fieldsPacket(
		rows.flatMap((row) => [row.side, row.strike, row.expiry, row.quantity]),
	);
}
