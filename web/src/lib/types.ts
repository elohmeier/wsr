export interface GameFile {
	name: string;
	size: number;
}
export interface Player {
	id: number;
	name: string;
	cash: number;
	computer: boolean;
}
export interface Company {
	id: number;
	name: string;
	symbol: string;
	industry: string;
	marketCapM: number;
	price: number | null;
	controller: number;
}
export interface Quote {
	slot: number;
	id: number;
	name: string;
	symbol: string;
	price: string;
	direction: number;
}
export interface Summary {
	deskLabels: Record<number, string>;
	newsHeading: [string, string];
	disabledControls: number[];
	version: string;
	year: number;
	quarter: number;
	startYear: number;
	endYear: number;
	currentPlayer: number;
	players: Player[];
	activeCorps: number;
	stockIndex: number;
	primeRate: number;
	gdpGrowth: number;
	topCorps: Company[];
	news: string[];
	gameDate: string;
	nextEarnings: string;
	controllerId: number;
	controllerName: string;
	tickerText: string;
	balance: {
		cash: number | null;
		otherAssets: number | null;
		totalAssets: number | null;
		debt: number | null;
		netWorth: number | null;
	} | null;
	economy: {
		oil: number | null;
		gold: number | null;
		silver: number | null;
		wheat: number | null;
		corn: number | null;
		longBondYield: number | null;
		shortBondYield: number | null;
	};
	gameInProgress: boolean;
	gameOver: boolean;
	clock: string;
	tickerSpeed: number;
	transactionsLeft: number;
	selectedEntity: number;
	selectedName: string;
	selectedSymbol: string;
	selectedPrice: string;
	selectedIndustry: number;
	activeEntity: number;
	activeName: string;
	industries: { id: number; name: string }[];
	watchlist: Quote[];
	currencySymbol: string;
	currencyCode: string;
	moneyUnit: string;
}
export interface MenuItem {
	group: string;
	id: number;
	label: string;
	disabled: boolean;
}
export interface HostRequest {
	kind: 'url' | 'program' | 'exit';
	value?: string;
}
export interface Result {
	ok: boolean;
	error?: string;
	summary?: Summary | null;
	files?: GameFile[];
	menus?: MenuItem[];
	countries?: string[];
	requests?: HostRequest[];
}
export interface Control {
	id: number;
	kind: 'label' | 'line' | 'button';
	text: string;
	x: number;
	y: number;
	w: number;
	h: number;
	style?: number;
	clickable?: boolean;
}
export interface GridRow {
	side: string;
	strike: string;
	expiry: string;
	quantity: string;
	price: string;
}
export interface FormField {
	id: number;
	label: string;
	value: string;
	hint: string;
	numeric: boolean;
	min: number | null;
	max: number | null;
	minLength: number | null;
	maxLength: number | null;
	choices: { value: string; label: string }[];
	refreshOnChange: boolean;
}
export type UIRequest = {
	token: number;
	title: string;
	helpTopic?: string | null;
} & (
	| { kind: 'msgbox'; text: string; style: number }
	| { kind: 'inputbox'; prompt: string; value: string }
	| {
			kind: 'symbolInput';
			prompt: string;
			value: string;
			companies: { id: number; name: string; symbol: string }[];
	  }
	| {
			kind: 'form';
			helpButton?: number | null;
			prompt: string[];
			fields: FormField[];
			buttons: { id: number; label: string }[];
	  }
	| { kind: 'choose'; prompt: string[]; options: string[]; disabled?: number[] }
	| {
			kind: 'select';
			prompt: string[];
			options: string[];
			selected: number | null;
			accept: string;
	  }
	| {
			kind: 'databaseSearch';
			prompt: string;
			count: string;
			focus: number | null;
			criteria: {
				id: number;
				label: string;
				checked: boolean;
				fields: { id: number; label: string; value: string }[];
				change: { id: number; label: string; value: string } | null;
			}[];
			buttons: { id: number; label: string }[];
	  }
	| {
			kind: 'report';
			lines: string[];
			yesNo: boolean;
			analystRating: number | null;
			ratingTrade: boolean;
			stockChart: boolean;
	  }
	| { kind: 'ownership'; width: number; height: number; controls: Control[] }
	| {
			kind: 'optionStation';
			stock: string;
			strikeHeader: string;
			expiryHeader: string;
			status: string;
			rows: GridRow[];
			buttons: { id: number; label: string }[];
	  }
	| {
			kind: 'chart';
			width: number;
			height: number;
			controls: Control[];
			tradeLabel: string | null;
			history: { month: number; high: number; low: number; close: number }[];
			entity: number;
			endYear: number;
			endMonth: number;
	  }
);
export interface StoredFile {
	name: string;
	bytes: Uint8Array | null;
}
export type Panel =
	| { kind: 'docs'; mode: 'help' | 'glossary' | 'manual'; query: string }
	| { kind: 'customizer' }
	| { kind: 'players' | 'companies' }
	| null;
