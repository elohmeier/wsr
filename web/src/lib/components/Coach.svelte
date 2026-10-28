<script lang="ts">
	import { onMount } from 'svelte';
	import { game, captureCoach, closeCoach } from '$lib/game';
	import {
		documentationSources,
		sourceFiles,
		tutorialSource,
	} from '$lib/coach/docs';
	import { MAX_QUESTION, validateConnection } from '$lib/coach/types';
	import {
		loadConnection,
		saveConnection,
		forgetConnection,
	} from '$lib/coach/connection';
	import type {
		ChatMessage,
		CoachEvent,
		Connection,
		MessageStatus,
		Snapshot,
		Source,
		ToolEntry,
	} from '$lib/coach/types';
	import * as MessageScroller from '$lib/components/ui/message-scroller';
	import * as Message from '$lib/components/ui/message';
	import * as Bubble from '$lib/components/ui/bubble';
	import * as InputGroup from '$lib/components/ui/input-group';
	import * as Marker from '$lib/components/ui/marker';
	import * as Trace from '$lib/components/ui/trace';
	import * as Dialog from '$lib/components/ui/dialog';
	import * as Sheet from '$lib/components/ui/sheet';
	import * as Field from '$lib/components/ui/field';
	import * as Alert from '$lib/components/ui/alert';
	import * as Empty from '$lib/components/ui/empty';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import { Checkbox } from '$lib/components/ui/checkbox';
	import { Spinner } from '$lib/components/ui/spinner';
	import CoachText from './CoachText.svelte';
	import CoachSource from './CoachSource.svelte';

	let desktop = $state(
		typeof window !== 'undefined' && matchMedia('(min-width: 1280px)').matches,
	);
	let draft = $state('');
	let provider = $state<Connection['provider']>('openrouter');
	let model = $state('');
	let baseUrl = $state('');
	let apiKey = $state('');
	let rememberApiKey = $state(false);
	let connectionMessage = $state('');
	let settings = $state(true);
	let busy = $state(false);
	let error = $state('');
	let notice = $state('');
	let messages = $state<ChatMessage[]>([]);
	let tools = $state<ToolEntry[]>([]);
	let documents = $state<Source[]>([]);
	let tutorials = $state<Source[]>([]);
	let readPaths = $state<string[]>([]);
	let snapshot = $state<Snapshot | null>(null);
	let sourceView = $state<{ source: Source; line: number } | null>(null);
	let worker: Worker | undefined;
	let runId = 0;
	let lastScope = '';
	let documentKey = '';
	const sources = $derived([...documents, ...tutorials]);
	const stale = $derived(
		!!snapshot &&
			(snapshot.sessionId !== $game.coachSession ||
				snapshot.revision !== $game.gameRevision),
	);
	const currentScope = $derived(
		`${$game.coachSession}:${$game.summary?.currentPlayer ?? 'none'}`,
	);

	onMount(() => {
		try {
			const saved = loadConnection(localStorage);
			if (saved) {
				provider = saved.provider;
				model = saved.model;
				baseUrl = saved.baseUrl;
				apiKey = saved.apiKey;
				rememberApiKey = saved.rememberApiKey;
				try {
					validateConnection(saved);
					settings = false;
				} catch {
					connectionMessage =
						'Connection settings restored. Enter your API key to continue.';
				}
			}
		} catch {
			connectionMessage =
				'Connection settings could not be restored. Enter them for this session.';
		}
		const media = matchMedia('(min-width: 1280px)');
		const update = () => {
			desktop = media.matches;
		};
		update();
		media.addEventListener('change', update);
		return () => {
			media.removeEventListener('change', update);
			worker?.terminate();
		};
	});
	$effect(() => {
		const key = $game.files
			.filter((f) => /^(HELP\.TXT|GLOSSARY\.TXT|WSRBOOK\.HTM)$/.test(f.name))
			.map((f) => `${f.name}:${f.size}`)
			.join('|');
		if (key !== documentKey) {
			documentKey = key;
			documents = documentationSources();
		}
	});
	$effect(() => {
		const scope = currentScope;
		if (lastScope && scope !== lastScope) {
			stop();
			messages = [];
			tools = [];
			readPaths = [];
			snapshot = null;
			sourceView = null;
			notice =
				'A different game or player is active. This is a new conversation.';
		}
		lastScope = scope;
	});

	function useConnection() {
		const connection: Connection = { provider, model, baseUrl, apiKey };
		try {
			validateConnection(connection);
		} catch (cause) {
			error =
				cause instanceof Error ? cause.message : 'Check the model connection.';
			return;
		}
		try {
			saveConnection(localStorage, connection, rememberApiKey);
			connectionMessage = rememberApiKey
				? 'Connection and API key saved on this device.'
				: 'Connection settings saved on this device. API key is not remembered.';
		} catch {
			connectionMessage =
				'Connection is available for this session, but could not be saved on this device.';
		}
		error = '';
		settings = false;
	}

	function forgetSavedConnection() {
		try {
			forgetConnection(localStorage);
		} catch {
			connectionMessage =
				'Could not forget the saved connection. Try again when browser storage is available.';
			return;
		}
		provider = 'openrouter';
		model = baseUrl = apiKey = '';
		rememberApiKey = false;
		settings = true;
		error = '';
		connectionMessage = 'Connection forgotten on this device.';
	}

	function stop() {
		++runId;
		worker?.postMessage({ type: 'stop' });
		worker?.terminate();
		worker = undefined;
		if (busy) notice = 'Response stopped. You can ask a follow-up.';
		busy = false;
		finishMessages('stopped');
		tools = tools.map((tool) =>
			tool.status === 'running'
				? { ...tool, status: 'error', output: 'Stopped.' }
				: tool,
		);
	}

	function finishMessages(status: MessageStatus) {
		messages = messages.map((message) =>
			message.status === 'streaming' ? { ...message, status } : message,
		);
	}

	async function submit(question = draft) {
		if (busy || !question.trim()) return;
		const connection: Connection = { provider, model, baseUrl, apiKey };
		try {
			validateConnection(connection);
		} catch (cause) {
			error =
				cause instanceof Error ? cause.message : 'Check the model connection.';
			settings = true;
			return;
		}
		if (question.length > MAX_QUESTION) {
			error = 'Keep your question below 4,000 characters.';
			return;
		}
		const id = ++runId;
		busy = true;
		error = '';
		notice = '';
		tools = [];
		readPaths = [];
		settings = false;
		snapshot = await captureCoach();
		if (id !== runId) return;
		const history = messages.map((message) => ({ ...message }));
		messages = [
			...messages,
			{
				id: `question-${id}`,
				role: 'user',
				text: question.trim(),
				sources: [],
				revision: snapshot?.revision ?? 0,
				gameDate: snapshot?.observation.context?.gameDate ?? '',
			},
		];
		draft = '';
		try {
			worker ??= new Worker(
				new URL('../coach/agent.worker.ts', import.meta.url),
				{ type: 'module' },
			);
			worker.onmessage = ({
				data,
			}: MessageEvent<CoachEvent & { runId: number }>) => {
				if (data.runId !== runId) return;
				if (data.type === 'text') {
					const found = messages.find((m) => m.id === data.id);
					if (found)
						messages = messages.map((m) =>
							m.id === data.id
								? {
										...m,
										text: data.text,
										status: data.status,
										sources: [...readPaths],
									}
								: m,
						);
					else
						messages = [
							...messages,
							{
								id: data.id,
								role: 'assistant',
								text: data.text,
								status: data.status,
								sources: [...readPaths],
								revision: snapshot?.revision ?? 0,
								gameDate: snapshot?.observation.context?.gameDate ?? '',
							},
						];
				} else if (data.type === 'tool') {
					const old = tools.find((tool) => tool.id === data.entry.id);
					const entry = {
						...data.entry,
						command: data.entry.command || old?.command || '',
					};
					tools = old
						? tools.map((tool) => (tool.id === entry.id ? entry : tool))
						: [...tools, entry];
				} else if (data.type === 'sources')
					readPaths = [...new Set([...readPaths, ...data.paths])];
				else if (data.type === 'done') {
					busy = false;
					finishMessages(
						data.error
							? /abort/i.test(data.error)
								? 'stopped'
								: 'error'
							: 'complete',
					);
					if (data.error && !/abort/i.test(data.error)) error = data.error;
				}
			};
			worker.onerror = () => {
				if (id === runId) {
					busy = false;
					finishMessages('error');
					error = 'The coach stopped. Try again; your game is unaffected.';
					worker?.terminate();
					worker = undefined;
				}
			};
			worker.postMessage({
				type: 'prompt',
				runId: id,
				question: question.trim(),
				history: JSON.parse(JSON.stringify(history)),
				snapshot: snapshot ? JSON.parse(JSON.stringify(snapshot)) : null,
				files: sourceFiles(sources),
				connection,
			});
		} catch (cause) {
			busy = false;
			finishMessages('error');
			error =
				cause instanceof Error ? cause.message : 'Unable to start the coach.';
		}
	}

	async function addTutorial(event: Event) {
		const input = event.currentTarget as HTMLInputElement;
		const file = input.files?.[0];
		if (!file) return;
		try {
			const source = await tutorialSource(file);
			tutorials = [...tutorials.filter((s) => s.path !== source.path), source];
			notice = `${source.title} is available for this browser session.`;
		} catch (cause) {
			error =
				cause instanceof Error
					? cause.message
					: 'Unable to read this tutorial.';
		}
		input.value = '';
	}
	function viewSource(source: Source, line = 1) {
		sourceView = { source, line };
	}
</script>

{#snippet content()}
	<div
		id="learning-coach"
		class="coach-content"
		role="group"
		aria-label="Learning coach"
	>
		<div class="coach-heading">
			<div>
				<h2>Learning coach</h2>
				<p>Understand the game. Make your own decisions.</p>
			</div>
			<Button
				variant="ghost"
				size="sm"
				onclick={closeCoach}
				aria-label="Close learning coach">Close</Button
			>
		</div>
		<div class="coach-body">
			<div class="coach-toolbar">
				<Button
					variant="outline"
					size="sm"
					onclick={() => (settings = !settings)}>Connection</Button
				>
				<Button
					variant="ghost"
					size="sm"
					disabled={busy || !messages.length}
					onclick={() => {
						stop();
						messages = [];
						tools = [];
						readPaths = [];
						notice = '';
						error = '';
					}}>New conversation</Button
				>
				<label class="coach-import"
					>Add tutorial<input
						id="coach-tutorial"
						type="file"
						accept=".txt,.md,.htm,.html"
						onchange={addTutorial}
						disabled={busy}
					/></label
				>
			</div>
			{#if settings}
				<form
					class="coach-settings"
					onsubmit={(event) => {
						event.preventDefault();
						useConnection();
					}}
				>
					<p>
						Your questions, retrieved documentation and visible game information
						are sent directly to your model provider. Connection settings are
						saved on this device when you choose Use connection. Nothing is sent
						until you ask.
					</p>
					<Field.Field
						><Field.Label for="coach-provider">Provider</Field.Label><select
							id="coach-provider"
							bind:value={provider}
							disabled={busy}
							><option value="openrouter">OpenRouter</option><option
								value="compatible">Compatible API endpoint</option
							></select
						></Field.Field
					>
					{#if provider === 'compatible'}<Field.Field
							><Field.Label for="coach-base-url">API base URL</Field.Label
							><Input
								id="coach-base-url"
								bind:value={baseUrl}
								placeholder="https://your-provider.example/v1"
								disabled={busy}
							/></Field.Field
						>{/if}
					<Field.Field
						><Field.Label for="coach-model">Model ID</Field.Label><Input
							id="coach-model"
							bind:value={model}
							placeholder="The model ID from your provider"
							disabled={busy}
						/></Field.Field
					>
					<Field.Field
						><Field.Label for="coach-api-key">Your API key</Field.Label><Input
							id="coach-api-key"
							type="password"
							bind:value={apiKey}
							autocomplete="off"
							spellcheck={false}
							disabled={busy}
						/></Field.Field
					>
					<Field.Field orientation="horizontal">
						<Checkbox
							id="coach-remember-key"
							bind:checked={rememberApiKey}
							disabled={busy}
						/>
						<Field.Content>
							<Field.Label for="coach-remember-key"
								>Remember API key on this device</Field.Label
							>
							<Field.Description
								>The key will be saved in this app’s browser storage. Otherwise
								it stays in memory for this session.</Field.Description
							>
						</Field.Content>
					</Field.Field>
					<div class="coach-connection-actions">
						<Button type="submit" size="sm" disabled={busy}
							>Use connection</Button
						>
						<Button
							type="button"
							variant="outline"
							size="sm"
							disabled={busy}
							onclick={forgetSavedConnection}>Forget connection</Button
						>
					</div>
				</form>
			{/if}
			{#if connectionMessage}<p class="coach-notice" role="status">
					{connectionMessage}
				</p>{/if}
			{#if error}<Alert.Root variant="destructive" class="coach-error"
					><Alert.Title>Coach needs attention</Alert.Title><Alert.Description
						>{error}</Alert.Description
					></Alert.Root
				>{/if}
			{#if notice}<p class="coach-notice" role="status">{notice}</p>{/if}
			{#if stale}<Marker.Root
					><Marker.Content
						>Game state changed. Your next question will use a fresh snapshot.</Marker.Content
					></Marker.Root
				>{/if}
			{#if snapshot?.pendingDialog}<p class="coach-notice">
					An engine dialog is open. State is from the last completed action; the
					current screen is included.
				</p>{/if}
			<MessageScroller.Provider autoScroll>
				<MessageScroller.Root class="coach-thread">
					<MessageScroller.Viewport aria-label="Coach conversation">
						<MessageScroller.Content aria-busy={busy}>
							{#if !messages.length}<Empty.Root class="coach-empty"
									><Empty.Header
										><Empty.Title
											>What would you like to understand?</Empty.Title
										><Empty.Description
											>Ask about a rule, the current screen, or how to approach
											your next decision. The coach reads documentation and
											visible state.</Empty.Description
										></Empty.Header
									></Empty.Root
								>{/if}
							{#each messages as message (message.id)}
								<MessageScroller.Item
									messageId={message.id}
									scrollAnchor={message.role === 'user'}
								>
									<Message.Root
										align={message.role === 'user' ? 'end' : 'start'}
									>
										<Message.Content>
											<Message.Header
												>{message.role === 'user'
													? 'You'
													: 'Coach'}{#if message.gameDate}<span
														class="coach-message-date"
														>{message.gameDate} · view {message.revision}</span
													>{/if}</Message.Header
											>
											{#if message.role === 'user'}<Bubble.Root
													variant="muted"
													align="end"
													><Bubble.Content
														><div class="coach-prose">
															{message.text}
														</div></Bubble.Content
													></Bubble.Root
												>{:else}<CoachText
													text={message.text}
													status={message.status}
													{sources}
													allowed={message.sources}
													onSource={viewSource}
												/>{/if}
										</Message.Content>
									</Message.Root>
								</MessageScroller.Item>
							{/each}
							{#if busy}<Marker.Root
									><Marker.Content
										><span class="coach-working"
											><Spinner />
											{tools.some((tool) => tool.status === 'running')
												? 'Reading sources and game state…'
												: 'Preparing an explanation…'}</span
										></Marker.Content
									></Marker.Root
								>{/if}
						</MessageScroller.Content>
					</MessageScroller.Viewport>
					<MessageScroller.Button behavior="auto" aria-label="Jump to latest" />
				</MessageScroller.Root>
			</MessageScroller.Provider>
			{#if tools.length}<Trace.Collapsible class="coach-tools"
					><Trace.CollapsibleTrigger
						><Trace.Line status={busy ? 'running' : undefined}
							><Trace.Title>Inspection details · {tools.length}</Trace.Title
							></Trace.Line
						></Trace.CollapsibleTrigger
					><Trace.CollapsibleContent
						><div class="coach-tool-scroll">
							{#each tools as tool (tool.id)}<Trace.Collapsible
									><Trace.CollapsibleTrigger
										><Trace.Line
											status={tool.status === 'running'
												? 'running'
												: tool.status === 'error'
													? 'error'
													: undefined}
											><Trace.Title><code>{tool.command}</code></Trace.Title
											></Trace.Line
										></Trace.CollapsibleTrigger
									><Trace.CollapsibleContent
										><pre>{tool.output ||
												'Running…'}</pre></Trace.CollapsibleContent
									></Trace.Collapsible
								>{/each}
						</div></Trace.CollapsibleContent
					></Trace.Collapsible
				>{/if}
			{#if !messages.length}<div class="coach-suggestions">
					{#each ['Explain this screen', 'Help me decide what to research next', 'Help me build a strategy'] as suggestion}<Button
							variant="outline"
							size="sm"
							onclick={() => (draft = suggestion)}>{suggestion}</Button
						>{/each}
				</div>{/if}
		</div>
		<form
			class="coach-composer"
			onsubmit={(event) => {
				event.preventDefault();
				void submit();
			}}
		>
			<label class="sr-only" for="coach-question">Your question</label>
			<InputGroup.Root>
				<InputGroup.Textarea
					id="coach-question"
					bind:value={draft}
					maxlength={MAX_QUESTION}
					placeholder="Ask about the game or your next decision…"
					rows={2}
					onkeydown={(event) => {
						if (
							event.key === 'Enter' &&
							!event.shiftKey &&
							!event.isComposing
						) {
							event.preventDefault();
							event.stopPropagation();
							if (!busy) void submit();
						}
					}}
				/>
				<InputGroup.Addon align="block-end"
					><span class="coach-composer-hint"
						>{documents.length
							? `${documents.length} local sections`
							: 'Import the game for its documentation'}</span
					>{#if busy}<InputGroup.Button
							type="button"
							onclick={stop}
							aria-label="Stop response">Stop</InputGroup.Button
						>{:else}<InputGroup.Button
							type="submit"
							variant="default"
							disabled={!draft.trim()}>Send</InputGroup.Button
						>{/if}</InputGroup.Addon
				>
			</InputGroup.Root>
		</form>
	</div>
{/snippet}

{#if $game.coachOpen}
	{#if $game.ui}
		<Dialog.Root
			open
			onOpenChange={(open) => {
				if (!open) closeCoach();
			}}
			><Dialog.Content class="coach-dialog" showCloseButton={false}
				><Dialog.Title class="sr-only">Learning coach</Dialog.Title
				><Dialog.Description class="sr-only"
					>The game dialog stays open while you ask for an explanation.</Dialog.Description
				>{@render content()}</Dialog.Content
			></Dialog.Root
		>
	{:else if desktop}
		<aside class="coach-dock" aria-label="Learning coach">
			{@render content()}
		</aside>
	{:else}
		<Sheet.Root
			open
			onOpenChange={(open) => {
				if (!open) closeCoach();
			}}
			><Sheet.Content side="right" class="coach-sheet" showCloseButton={false}
				><Sheet.Title class="sr-only">Learning coach</Sheet.Title
				><Sheet.Description class="sr-only"
					>Ask questions about game mechanics and strategy.</Sheet.Description
				>{@render content()}</Sheet.Content
			></Sheet.Root
		>
	{/if}
{/if}
{#if sourceView}<Dialog.Root
		open
		onOpenChange={(open) => {
			if (!open) sourceView = null;
		}}
		><Dialog.Content class="coach-source-dialog" showCloseButton={false}
			><Dialog.Header
				><Dialog.Title>{sourceView.source.title}</Dialog.Title
				><Dialog.Description
					>{sourceView.source.path} · from line {sourceView.line}</Dialog.Description
				></Dialog.Header
			>
			<CoachSource source={sourceView.source} line={sourceView.line} />
			<Dialog.Footer
				><Button variant="outline" onclick={() => (sourceView = null)}
					>Close source</Button
				></Dialog.Footer
			></Dialog.Content
		></Dialog.Root
	>{/if}
