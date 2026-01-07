// Utility functions
import type { FileInfo, TreeNodeWithCoverage } from "./types";

export function buildFileTree(files: FileInfo[]): TreeNodeWithCoverage {
	const root: TreeNodeWithCoverage = {
		name: "",
		children: {},
		files: [],
		totalUnits: 0,
		coveredUnits: 0,
	};

	for (const file of files) {
		const parts = file.path.split("/");
		let current = root;

		for (let i = 0; i < parts.length - 1; i++) {
			const part = parts[i];
			if (!current.children[part]) {
				current.children[part] = {
					name: part,
					children: {},
					files: [],
					totalUnits: 0,
					coveredUnits: 0,
				};
			}
			current = current.children[part];
		}

		current.files.push({ ...file, name: parts[parts.length - 1] });
	}

	// Compute folder coverage recursively
	function computeCoverage(node: TreeNodeWithCoverage): void {
		let total = 0;
		let covered = 0;

		// Add files in this folder
		for (const f of node.files) {
			total += f.totalUnits || 0;
			covered += f.coveredUnits || 0;
		}

		// Add children folders
		for (const child of Object.values(node.children)) {
			computeCoverage(child);
			total += child.totalUnits;
			covered += child.coveredUnits;
		}

		node.totalUnits = total;
		node.coveredUnits = covered;
	}

	computeCoverage(root);
	return root;
}

export function getCoverageBadge(
	covered: number,
	total: number,
): { class: string; text: string } {
	if (total === 0) return { class: "none", text: "-" };
	const pct = (covered / total) * 100;
	if (pct === 100) return { class: "full", text: "100%" };
	if (pct >= 50) return { class: "partial", text: `${Math.round(pct)}%` };
	return { class: "none", text: `${Math.round(pct)}%` };
}

export function getStatClass(pct: number): string {
	if (pct >= 80) return "good";
	if (pct >= 50) return "warn";
	return "bad";
}

// Render rule text with backticks -> <code> and RFC 2119 keywords highlighted
export function renderRuleText(text: string | undefined): string {
	if (!text) return "";

	// Escape HTML first
	let result = text
		.replace(/&/g, "&amp;")
		.replace(/</g, "&lt;")
		.replace(/>/g, "&gt;");

	// Process `code` (backticks)
	let inCode = false;
	let processed = "";
	for (const char of result) {
		if (char === "`") {
			if (inCode) {
				processed += "</code>";
				inCode = false;
			} else {
				processed += "<code>";
				inCode = true;
			}
		} else {
			processed += char;
		}
	}
	if (inCode) processed += "</code>";
	result = processed;

	// Wrap RFC 2119 keywords (order matters - longer phrases first)
	result = result
		.replace(/\bMUST NOT\b/g, "<kw-must-not>MUST NOT</kw-must-not>")
		.replace(/\bSHALL NOT\b/g, "<kw-shall-not>SHALL NOT</kw-shall-not>")
		.replace(/\bSHOULD NOT\b/g, "<kw-should-not>SHOULD NOT</kw-should-not>")
		.replace(
			/\bNOT RECOMMENDED\b/g,
			"<kw-not-recommended>NOT RECOMMENDED</kw-not-recommended>",
		)
		.replace(/\bMUST\b/g, "<kw-must>MUST</kw-must>")
		.replace(/\bREQUIRED\b/g, "<kw-required>REQUIRED</kw-required>")
		.replace(/\bSHALL\b/g, "<kw-shall>SHALL</kw-shall>")
		.replace(/\bSHOULD\b/g, "<kw-should>SHOULD</kw-should>")
		.replace(/\bRECOMMENDED\b/g, "<kw-recommended>RECOMMENDED</kw-recommended>")
		.replace(/\bMAY\b/g, "<kw-may>MAY</kw-may>")
		.replace(/\bOPTIONAL\b/g, "<kw-optional>OPTIONAL</kw-optional>");

	return result;
}

// Split highlighted HTML into self-contained lines
// Each line will have properly balanced open/close tags
export function splitHighlightedHtml(html: string) {
	// Use DOMParser for robust HTML parsing
	const parser = new DOMParser();
	const doc = parser.parseFromString(`<div>${html}</div>`, "text/html");
	const container = doc.body.firstChild;

	const lines: string[] = [];
	let currentLine = "";
	const openTags: { tag: string; attrs: string }[] = [];

	function processNode(node: Node) {
		if (node.nodeType === Node.TEXT_NODE) {
			const text = node.textContent || "";
			for (const char of text) {
				if (char === "\n") {
					// Close tags, push line, reopen tags
					for (let j = openTags.length - 1; j >= 0; j--) {
						currentLine += `</${openTags[j].tag}>`;
					}
					lines.push(currentLine);
					currentLine = "";
					for (const t of openTags) {
						currentLine += `<${t.tag}${t.attrs}>`;
					}
				} else {
					currentLine +=
						char === "<"
							? "&lt;"
							: char === ">"
								? "&gt;"
								: char === "&"
									? "&amp;"
									: char;
				}
			}
		} else if (node.nodeType === Node.ELEMENT_NODE) {
			const el = node as Element;
			const tag = el.tagName.toLowerCase();
			let attrs = "";
			for (const attr of el.attributes) {
				attrs += ` ${attr.name}="${attr.value.replace(/"/g, "&quot;")}"`;
			}

			currentLine += `<${tag}${attrs}>`;
			openTags.push({ tag, attrs });

			for (const child of el.childNodes) {
				processNode(child);
			}

			openTags.pop();
			currentLine += `</${tag}>`;
		}
	}

	if (container) {
		for (const child of container.childNodes) {
			processNode(child);
		}
	}

	// Push final line if any content remains
	if (currentLine) {
		lines.push(currentLine);
	}

	return lines;
}

// Helper to split file path into dir and filename
export function splitPath(filePath: string) {
	const lastSlash = filePath.lastIndexOf("/");
	if (lastSlash === -1) return { dir: "", name: filePath };
	return {
		dir: filePath.slice(0, lastSlash + 1),
		name: filePath.slice(lastSlash + 1),
	};
}
