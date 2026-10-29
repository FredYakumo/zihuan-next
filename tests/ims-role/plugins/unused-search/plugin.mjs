// Satisfies the role's required search connection; these cases must never search.
export const plugin_manifest = { plugin_type: "web_search_engine", name: "unused-search" };
export function search() { throw new Error("Unexpected web search in IMS behaviour test"); }
