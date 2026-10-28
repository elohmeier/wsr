<script lang="ts">
	import * as Field from '$lib/components/ui/field';
	import * as NativeSelect from '$lib/components/ui/native-select';
	import { Input } from '$lib/components/ui/input';
	import { Button } from '$lib/components/ui/button';
	import { Checkbox } from '$lib/components/ui/checkbox';
	import { game, resume, download } from '$lib/game';
	let { open = $bindable(true) } = $props();
	let selected = $state(''),
		fixed = $state(false),
		entered = $state(false);
	const saves = $derived(
		$game.files.filter((f) => /^GAME\d{2}\.DAT$/.test(f.name)),
	);
	const prepared = $derived(
		[
			'WSR101.DAT',
			'MSCNEWS.DAT',
			'SCORES.WSR',
			'CORPNAME.DAT',
			'ENGINE-CONTENT.WSR',
		].every((n) => $game.files.some((f) => f.name === n)),
	);
	$effect(() => {
		if (saves.length && !saves.some((f) => f.name === selected))
			selected = saves[0].name;
	});
	$effect(() => {
		if ($game.summary && !entered) {
			entered = true;
			open = false;
		}
	});
	async function upload(event: Event, installer: boolean) {
		const input = event.currentTarget as HTMLInputElement,
			file = input.files?.[0];
		if (file)
			await game.run({
				cmd: installer ? 'installer' : 'load',
				name: file.name,
				bytes: new Uint8Array(await file.arrayBuffer()),
				fixed,
			});
		input.value = '';
	}
</script>

<details id="files-panel" bind:open class="border bg-card">
	<summary class="cursor-pointer px-5 py-4 font-medium"
		>Game & saved games <span id="file-count" class="ml-2 text-muted-foreground"
			>{prepared ? 'Ready' : ''}</span
		></summary
	>
	<div class="flex flex-col gap-5 px-4 pt-2 pb-4">
		<Field.Group class="grid gap-4 md:grid-cols-2">
			<Field.Field
				><Field.Label for="installer">Open wsr985.exe</Field.Label><Input
					id="installer"
					type="file"
					accept=".exe"
					disabled={!$game.ready || $game.busy}
					onchange={(e) => upload(e, true)}
				/></Field.Field
			>
			<Field.Field
				><Field.Label for="save-file">Open a saved game</Field.Label><Input
					id="save-file"
					type="file"
					accept=".dat"
					disabled={!$game.ready || $game.busy}
					onchange={(e) => upload(e, false)}
				/></Field.Field
			>
		</Field.Group>
		<div class="flex flex-wrap items-end gap-3">
			<Field.Field class="w-auto"
				><Field.Label for="saved-games">Saved in this browser</Field.Label>
				<NativeSelect.Root
					id="saved-games"
					bind:value={selected}
					disabled={$game.busy}
				>
					{#if !saves.length}<NativeSelect.Option value=""
							>No saved games yet</NativeSelect.Option
						>{/if}
					{#each saves as file (file.name)}<NativeSelect.Option
							value={file.name}>{file.name}</NativeSelect.Option
						>{/each}
				</NativeSelect.Root>
			</Field.Field>
			<Button
				id="resume"
				variant="outline"
				disabled={!$game.ready || !selected || $game.busy}
				onclick={() => resume(selected, fixed)}>Open</Button
			>
			<Button
				id="download"
				variant="outline"
				disabled={!selected || $game.busy}
				onclick={() => download(selected)}>Download file</Button
			>
			<Field.Field orientation="horizontal" class="w-auto self-center"
				><Checkbox id="fixed" bind:checked={fixed} /><Field.Label for="fixed"
					>Repair legacy save decoder</Field.Label
				></Field.Field
			>
		</div>
		<p id="file-status" role="status" class="text-sm text-muted-foreground">
			{prepared
				? 'Ready to play. Choose New game or open a saved game. Saves stay in this browser.'
				: 'Choose wsr985.exe above to prepare the game.'}
		</p>
	</div>
</details>
