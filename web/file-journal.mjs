// Keep the newest version of every changed file until durable storage acknowledges it.
// A late acknowledgement must never remove a newer save of the same slot.
export class FileJournal {
	#revision = 0;
	#files = new Map();
	record(files) {
		for (const file of files)
			this.#files.set(file.name, { ...file, revision: ++this.#revision });
	}
	acknowledge(revision) {
		for (const [name, file] of this.#files)
			if (file.revision <= revision) this.#files.delete(name);
	}
	snapshot() {
		return {
			revision: this.#revision,
			changes: [...this.#files.values()].map(({ name, bytes }) => ({
				name,
				bytes,
			})),
		};
	}
}
