// Route types
export type ViewType = "sources" | "spec" | "coverage";

export interface SourcesRoute {
	view: "sources";
	spec: string | null;
	file: string | null;
	line: number | null;
	context: string | null;
}

export interface SpecRoute {
	view: "spec";
	spec: string | null;
	rule: string | null;
	heading: string | null;
}

export interface CoverageRoute {
	view: "coverage";
	spec: string | null;
	filter: string | null;
	level: string | null;
}

export type Route = SourcesRoute | SpecRoute | CoverageRoute;

// API data types
export interface FileRef {
	file: string;
	line: number;
}

export interface Rule {
	id: string;
	text?: string;
	level?: string;
	implRefs: FileRef[];
	verifyRefs: FileRef[];
}

export interface Spec {
	name: string;
	rules: Rule[];
}

export interface FileInfo {
	path: string;
	coveredUnits: number;
	totalUnits: number;
}

export interface Config {
	projectRoot?: string;
	specs?: { name: string }[];
}

export interface ForwardData {
	specs: Spec[];
}

export interface ReverseData {
	files: FileInfo[];
	totalUnits: number;
	coveredUnits: number;
}

export interface ApiData {
	config: Config;
	forward: ForwardData;
	reverse: ReverseData;
}

export interface FileContent {
	path: string;
	content: string;
	/** Server-side syntax highlighted HTML */
	html: string;
	units: CodeUnit[];
}

export interface CodeUnit {
	kind: string;
	name: string | null;
	startLine: number;
	endLine: number;
	ruleRefs: string[];
}

export interface SpecSection {
	sourceFile: string;
	html: string;
}

export interface OutlineCoverage {
	implCount: number;
	verifyCount: number;
	total: number;
}

export interface OutlineEntry {
	title: string;
	slug: string;
	level: number;
	coverage: OutlineCoverage;
	aggregated: OutlineCoverage;
}

export interface SpecContent {
	name: string;
	sections: SpecSection[];
	outline: OutlineEntry[];
}

// Search types
export interface SearchResult {
	kind: "source" | "rule";
	id: string;
	line: number;
	content: string;
	highlighted: string;
	score: number;
}

export interface SearchResults {
	results: SearchResult[];
	query: string;
}

// Tree types
export interface TreeNode {
	name: string;
	files: FileInfo[];
	children: Record<string, TreeNode>;
}

// Editor types
export interface Editor {
	name: string;
	urlTemplate: (path: string, line: number) => string;
	devicon?: string;
	icon?: string;
}

// Level config
export interface LevelConfig {
	name: string;
	dotClass: string;
}

// Tree node with coverage info
export interface TreeNodeWithCoverage extends TreeNode {
	totalUnits: number;
	coveredUnits: number;
	files: FileInfoWithName[];
	children: Record<string, TreeNodeWithCoverage>;
}

export interface FileInfoWithName extends FileInfo {
	name: string;
}

// Component Props
export interface FileTreeProps {
	node: TreeNodeWithCoverage;
	selectedFile: string | null;
	onSelectFile: (
		path: string,
		line?: number | null,
		context?: string | null,
	) => void;
	depth?: number;
	search?: string;
	parentPath?: string;
}

export interface FileTreeFileProps {
	file: FileInfoWithName;
	selected: boolean;
	onClick: () => void;
}

export interface SearchResultItemProps {
	result: SearchResult;
	isSelected: boolean;
	onSelect: () => void;
	onHover: () => void;
}

export interface SearchModalProps {
	onClose: () => void;
	onSelect: (result: SearchResult) => void;
}

export interface HeaderProps {
	view: ViewType;
	search: string;
	onSearchChange: (search: string) => void;
	onViewChange: (view: ViewType) => void;
	onOpenSearch: () => void;
}

export interface FilePathProps {
	file: string;
	line?: number | null;
	short?: boolean;
	type?: "impl" | "verify" | "source";
	onClick?: () => void;
	className?: string;
}

export interface LangIconProps {
	filePath: string;
	className?: string;
}

export interface LucideIconProps {
	name: string;
	className?: string;
}

export interface CoverageViewProps {
	data: ForwardData;
	config: Config;
	search: string;
	onSearchChange: (search: string) => void;
	level: string;
	onLevelChange: (level: string) => void;
	filter: string | null;
	onFilterChange: (filter: string | null) => void;
	onSelectRule: (ruleId: string) => void;
	onSelectFile: (
		path: string,
		line?: number | null,
		context?: string | null,
	) => void;
}

export interface SourcesViewProps {
	data: ReverseData;
	forward: ForwardData;
	config: Config;
	search: string;
	selectedFile: string | null;
	selectedLine: number | null;
	ruleContext: string | null;
	onSelectFile: (
		path: string,
		line?: number | null,
		context?: string | null,
	) => void;
	onSelectRule: (ruleId: string) => void;
	onClearContext: () => void;
}

export interface SpecViewProps {
	config: Config;
	forward: ForwardData;
	version: string | null;
	selectedSpec: string | null;
	selectedRule: string | null;
	selectedHeading: string | null;
	onSelectSpec: (name: string) => void;
	onSelectRule: (ruleId: string) => void;
	onSelectFile: (
		path: string,
		line?: number | null,
		context?: string | null,
	) => void;
	scrollPosition: number;
	onScrollChange: (pos: number) => void;
}

export interface CodeViewProps {
	file: FileContent;
	config: Config;
	selectedLine: number | null;
	onSelectRule: (ruleId: string) => void;
}

export interface FileRefProps {
	file: string;
	line: number;
	type: "impl" | "verify";
	onSelectFile: (path: string, line?: number | null) => void;
}
