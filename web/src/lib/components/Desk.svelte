<script lang="ts">
	import * as Field from '$lib/components/ui/field';
	import * as NativeSelect from '$lib/components/ui/native-select';
	import { Input } from '$lib/components/ui/input';
	import { Button } from '$lib/components/ui/button';
	import { game, openList } from '$lib/game';
	import { cp1252, cleanLabel } from '$lib/encoding';
	import DeskButton from './DeskButton.svelte';
	let symbol = $state('');
	const s = $derived($game.summary!);
	const label = (id: number) =>
		cleanLabel(s.deskLabels[id] ?? '')
			.replace(/\.{2,}$/, '')
			.trim();
	const research = [170, 175, 102, 168, 173, 104];
	const trades = [105, 107, 108, 106, 109, 110];
	const quick = [130, 128, 125, 131, 132, 126, 127, 129, 134];
	const money = (n: number | null | undefined) =>
		n == null
			? '—'
			: n.toLocaleString(undefined, {
					minimumFractionDigits: 2,
					maximumFractionDigits: 2,
				});
	const spot = (n: number | null) => (n == null ? '—' : '$' + money(n));
	const rate = (n: number | null) => (n == null ? '—' : money(n) + '%');
	const indicators = $derived([
		{ id: 114, label: label(117), value: money(s.stockIndex) },
		{ id: 152, label: label(116), value: rate(s.primeRate) },
		{ id: 157, label: label(155), value: rate(s.economy.longBondYield) },
		{ id: 158, label: label(156), value: rate(s.economy.shortBondYield) },
		{ id: 154, label: label(118), value: spot(s.economy.oil), monetary: true },
		{
			id: 161,
			label: label(162),
			value: spot(s.economy.silver),
			monetary: true,
		},
		{ id: 159, label: label(160), value: spot(s.economy.gold), monetary: true },
		{
			id: 163,
			label: label(164),
			value: spot(s.economy.wheat),
			monetary: true,
		},
		{ id: 153, label: label(115), value: rate(s.gdpGrowth) },
		{ id: 165, label: label(166), value: spot(s.economy.corn), monetary: true },
	]);
	const balances = $derived([
		[label(139), s.balance?.cash],
		[label(138), s.balance?.otherAssets],
		[label(137), s.balance?.totalAssets],
		[
			label(136),
			s.balance?.debt == null
				? null
				: s.balance.debt === 0
					? 0
					: -s.balance.debt,
		],
	] as const);
</script>

<section id="desk" class="wsr-workspace" aria-label="Trading desk">
	<div class="wsr-left-column">
		<section class="wsr-panel wsr-entity" aria-labelledby="entity-heading">
			<h2 id="entity-heading">{label(111)}</h2>
			<div class="wsr-panel-content">
				<h3 id="selected-name">
					{s.selectedName || label(169)}
				</h3>
				<form
					id="symbol-form"
					onsubmit={(e) => {
						e.preventDefault();
						game.action(4, 0, cp1252(symbol));
					}}
				>
					<Field.Group
						><Field.Field orientation="horizontal"
							><Field.Label for="symbol">{label(142)}</Field.Label><Input
								id="symbol"
								bind:value={symbol}
								aria-label="Stock symbol or player number"
								readonly={!$game.ready ||
									$game.busy ||
									s.disabledControls.includes(202)}
								autocomplete="off"
								placeholder={s.selectedSymbol || 'e.g. IBM'}
							/><Button
								type="submit"
								disabled={!$game.ready ||
									$game.busy ||
									s.disabledControls.includes(1)}>{label(1)}</Button
							></Field.Field
						></Field.Group
					>
				</form>
				<dl class="wsr-readouts">
					<div>
						<dt>{label(151)}</dt>
						<dd id="selected-details" class="wsr-money">
							{s.selectedEntity > 10
								? s.currencySymbol.trim() + s.selectedPrice
								: 'N/A'}
						</dd>
					</div>
					<div>
						<dt>{label(148)}</dt>
						<dd>
							{#if s.controllerId}<DeskButton control={181}
									>{s.controllerName}</DeskButton
								>{:else}<span class="wsr-readout">{s.controllerName}</span>{/if}
						</dd>
					</div>
					<div>
						<dt>{label(145)}</dt>
						<dd>
							<DeskButton control={190} id="active-name"
								>{s.activeName ||
									s.players.find((p) => p.id === s.currentPlayer)?.name ||
									'Player'}</DeskButton
							>
						</dd>
					</div>
				</dl>
				<Field.Group
					><Field.Field
						><Field.Label for="industry">Industry group</Field.Label
						><NativeSelect.Root
							id="industry"
							value={String(s.selectedIndustry)}
							disabled={!$game.ready ||
								$game.busy ||
								s.disabledControls.includes(234)}
							onchange={(e) =>
								game.action(7, +(e.currentTarget as HTMLSelectElement).value)}
							class="w-full"
							><NativeSelect.Option value="0"
								>Select industry</NativeSelect.Option
							>{#each s.industries as industry (industry.id)}<NativeSelect.Option
									value={String(industry.id)}
									>{industry.name}</NativeSelect.Option
								>{/each}</NativeSelect.Root
						></Field.Field
					></Field.Group
				>
			</div>
		</section>
		<section
			class="wsr-panel"
			id="balance-sheet"
			aria-labelledby="balance-heading"
		>
			<h2 id="balance-heading">
				{label(122).replace(/\([^)]*\)/, '(' + s.moneyUnit + 'illions)')}
			</h2>
			<div class="wsr-panel-content">
				<dl class="wsr-readouts wsr-balance">
					{#each balances as [label, value] (label)}<div>
							<dt>{label}</dt>
							<dd class="wsr-money">{money(value)}</dd>
						</div>{/each}
					<div class="wsr-net-worth">
						<dt>{label(135)}</dt>
						<dd class="wsr-money">
							<DeskButton control={218}>{money(s.balance?.netWorth)}</DeskButton
							>
						</dd>
					</div>
				</dl>
			</div>
		</section>
		<section class="wsr-panel" id="market" aria-labelledby="economy-heading">
			<h2 id="economy-heading">{label(113)}</h2>
			<div class="wsr-panel-content">
				<dl class="wsr-economy">
					{#each indicators as item (item.id)}<div>
							<dt>{item.label}</dt>
							<dd class:wsr-money={item.monetary}>
								<DeskButton control={item.id}>{item.value}</DeskButton>
							</dd>
						</div>{/each}
				</dl>
				<p class="wsr-caption">
					Spot prices in USD · Select a value for its chart
				</p>
			</div>
		</section>
	</div>
	<div class="wsr-main-column">
		<div class="wsr-command-deck">
			<section class="wsr-panel">
				<h2>{label(103)}</h2>
				<div class="wsr-panel-content wsr-button-grid">
					{#each research as control (control)}<DeskButton {control}
							>{label(control)}</DeskButton
						>{/each}
				</div>
			</section>
			<section class="wsr-panel">
				<h2>{label(101)}</h2>
				<div class="wsr-panel-content wsr-button-grid">
					{#each trades as control (control)}<DeskButton {control}
							>{label(control)}</DeskButton
						>{/each}
				</div>
			</section>
			<section class="wsr-panel">
				<h2>{label(100)}</h2>
				<div class="wsr-panel-content wsr-other-grid">
					<DeskButton control={244} primary>{label(244)}</DeskButton><DeskButton
						control={246}>{label(246)}</DeskButton
					><DeskButton control={247}>{label(247)}</DeskButton><DeskButton
						control={248}>{label(248)}</DeskButton
					>
				</div>
			</section>
		</div>
		<div class="wsr-center-column">
			<section class="wsr-status-strip" aria-label="Game status">
				<div>
					<span>{label(141).split(':')[0]}:</span><strong>{s.gameDate}</strong>
				</div>
				<div>
					<span>Next Earnings Release Date:</span><strong
						>{s.nextEarnings || 'N/A'}</strong
					>
				</div>
				<div>
					<span>{label(112).split(':')[0]}:</span><strong id="turn-info"
						>{s.gameOver ? 'Game over' : s.transactionsLeft}</strong
					>
				</div>
				<div>
					<span>{label(140).split(':')[0]}:</span><strong
						>{s.players.find((p) => p.id === s.currentPlayer)?.name}</strong
					>
				</div>
			</section>

			<section class="wsr-panel wsr-news-panel" aria-labelledby="news-heading">
				<h2 id="news-heading">
					<span>{s.newsHeading[0]}</span>{s.newsHeading[1]}
				</h2>
				<div class="wsr-news-list" role="log" aria-label="Market news">
					<ul id="news">
						{#each s.news as headline, i (i)}<li>
								{headline}
							</li>{/each}{#if !s.news.length}<li class="wsr-caption">
								Market headlines will appear here as the game progresses.
							</li>{/if}
					</ul>
				</div>
			</section>
			<section class="wsr-ticker" aria-label="Stock ticker">
				<DeskButton control={249} id="ticker-toggle">{label(249)}</DeskButton>
				<p id="ticker-text" title={s.tickerText}>
					{s.tickerText || 'Start the ticker to follow market activity.'}
				</p>
				<span
					class="wsr-clock"
					data-running={s.clock === 'RUN'}
					aria-label={s.clock === 'RUN' ? 'Market running' : 'Market paused'}
					>{s.clock === 'RUN' ? 'RUN' : 'PAUSED'}</span
				>
			</section>
			<section class="wsr-panel wsr-quick-panel">
				<h2>{label(124)}</h2>
				<div class="wsr-panel-content wsr-button-grid">
					{#each quick as control (control)}<DeskButton {control}
							>{label(control)}</DeskButton
						>{/each}
				</div>
				<div class="wsr-secondary-links">
					<Button
						variant="ghost"
						data-players
						onclick={() => openList('players')}
						disabled={$game.busy}>Players & standings</Button
					><Button
						variant="ghost"
						data-companies
						onclick={() => openList('companies')}
						disabled={$game.busy}>Largest companies</Button
					><Button
						variant="ghost"
						onclick={() => game.report(3)}
						disabled={$game.busy}>Economy report</Button
					>
				</div>
			</section>
		</div>
		<section class="wsr-panel wsr-quotes" aria-labelledby="watchlist-heading">
			<h2 id="watchlist-heading">{label(950)}</h2>
			<div class="wsr-watch-controls">
				<DeskButton control={951}>{label(951)}</DeskButton><DeskButton
					control={954}>{label(954)}</DeskButton
				><DeskButton control={953}>{label(953)}</DeskButton>
			</div>
			{#if !s.watchlist.length}<p id="watchlist-empty" class="wsr-quote-empty">
					Select a company, then add it to follow its price here. Up to 15
					companies.
				</p>{/if}
			<table
				id="watchlist-table"
				class="wsr-quote-table"
				hidden={!s.watchlist.length}
			>
				<thead><tr><th>Symbol / company</th><th>Price</th></tr></thead><tbody
					id="watchlist"
					>{#each s.watchlist as quote (quote.id)}<tr
							class:quote-up={quote.direction > 0}
							class:quote-down={quote.direction < 0}
							><td
								><DeskButton control={800 + quote.slot}
									>{quote.symbol}</DeskButton
								><span class="wsr-quote-name">{quote.name}</span></td
							><td class="wsr-quote-price"
								><span class="wsr-money"
									>{s.currencySymbol.trim()}{quote.price || '—'}</span
								><span class="wsr-quote-direction"
									>{quote.direction > 0
										? '↑ Up'
										: quote.direction < 0
											? '↓ Down'
											: '— Unchanged'}</span
								></td
							></tr
						>{/each}</tbody
				>
			</table>
			<p class="wsr-quotes-footer">
				{s.watchlist.length} / 15 stocks · {s.activeCorps} active companies
			</p>
		</section>
	</div>
</section>
