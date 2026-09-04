import fs from "node:fs";
import { promises as fsPromises } from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { createZihuanSdk, hydrateResources } from "./zihuan_sdk.mjs";

const engineDirectory = path.dirname(fileURLToPath(import.meta.url));
const nodeDirectory = path.resolve(engineDirectory, "../dag_nodes");
const argument = process.argv[2];
let requestId = 0;
const pendingHostCalls = new Map();

/**
 * Recursively finds dynamic script node modules in a deterministic order.
 *
 * @param {string} directory Absolute directory to scan.
 * @returns {Promise<string[]>} Absolute paths to `.mjs` node modules.
 */
async function nodeModuleFiles(directory) {
    const entries = await fsPromises.readdir(directory, { withFileTypes: true });
    const files = [];
    for (const entry of entries.sort((left, right) => left.name.localeCompare(right.name))) {
        const file = path.join(directory, entry.name);
        if (entry.isDirectory()) {
            files.push(...await nodeModuleFiles(file));
        } else if (entry.isFile() && entry.name.endsWith(".mjs")) {
            files.push(file);
        }
    }
    return files;
}

/**
 * Loads and validates every node definition exported by the dynamic script modules.
 *
 * A module may omit `nodes`; exported definitions must have a unique `type_id`
 * and an executable `execute` function.
 *
 * @returns {Promise<NodeDefinition[]>} Registered node definitions.
 * @throws {Error} When a module cannot load or the catalog is invalid.
 */
async function loadNodes() {
    const catalog = [];
    const diagnostics = [];
    for (const file of await nodeModuleFiles(nodeDirectory)) {
        let module;
        try {
            module = await import(pathToFileURL(file).href);
        } catch (error) {
            diagnostics.push({ language: "javascript", message: `failed to load ${path.relative(nodeDirectory, file)}: ${error.message}` });
            continue;
        }
        if (module.nodes === undefined) continue;
        if (!Array.isArray(module.nodes)) {
            diagnostics.push({ language: "javascript", message: `${path.relative(nodeDirectory, file)} must export a nodes array` });
            continue;
        }
        // 为每个节点添加 script_path
        const scriptPath = path.relative(process.cwd(), file);
        const nodesWithScriptPath = module.nodes.map(node => ({
            ...node,
            script_path: scriptPath
        }));
        catalog.push(...nodesWithScriptPath);
    }
    const seen = new Set();
    for (const node of catalog) {
        if (!node || typeof node.type_id !== "string" || node.type_id.trim() === "" || typeof node.execute !== "function") {
            throw new Error("DAG node catalog contains invalid node metadata");
        }
        if (seen.has(node.type_id)) throw new Error(`duplicate DAG node type_id: ${node.type_id}`);
        seen.add(node.type_id);
    }
    return { nodes: catalog, diagnostics };
}

/**
 * Sends a ZiHuan runtime API request to the Rust host and waits for its matching response.
 *
 * The `--serve` protocol writes the request to stdout. Rust replies on stdin
 * with a `host_response` carrying the same generated request ID.
 *
 * @param {string} method Host capability name, such as `model.llm_infer`.
 * @param {Record<string, unknown>} [params={}] Capability arguments.
 * @returns {Promise<unknown>} Result supplied by the Rust host.
 */
function hostCall(method, params = {}) {
    const id = ++requestId;
    process.stdout.write(`${JSON.stringify({ kind: "host_request", id, method, params })}\n`);
    return new Promise((resolve, reject) => pendingHostCalls.set(id, { resolve, reject }));
}

/**
 * Builds the context passed to a node's `execute` implementation.
 *
 * Resource handles are hydrated before the node sees them so node code can
 * use their typed handle classes instead of wire-format JSON objects.
 *
 * @param {Record<string, unknown>} request Execute request from the Rust host.
 * @returns {import("./zihuan_sdk.d.ts").NodeExecutionContext} Node execution context.
 */
function executionContext(request) {
    return {
        nodeId: request.node_id,
        nodeName: request.node_name,
        inputs: hydrateResources(request.inputs ?? {}),
        inline_values: request.inline_values ?? {},
        zihuan: createZihuanSdk(hostCall),
    };
}

const { nodes, diagnostics } = await loadNodes();
const nodeByType = new Map(nodes.map((node) => [node.type_id, node]));

/**
 * Resolves a node's static or inline-configuration-dependent ports.
 *
 * @param {NodeDefinition} node Node definition to inspect.
 * @param {Record<string, unknown>} inlineValues Persisted inline configuration.
 * @returns {{ input_ports: unknown[], output_ports: unknown[] }} Resolved port lists.
 * @throws {Error} When a dynamic port resolver returns non-array values.
 */
function resolvedPorts(node, inlineValues) {
    const resolved = node.resolve_ports?.({ inline_values: inlineValues ?? {} }) ?? {};
    const inputPorts = resolved.input_ports ?? node.input_ports ?? [];
    const outputPorts = resolved.output_ports ?? node.output_ports ?? [];
    if (!Array.isArray(inputPorts) || !Array.isArray(outputPorts)) {
        throw new Error(`DAG node ${node.type_id} resolve_ports must return input_ports and output_ports arrays`);
    }
    return { input_ports: inputPorts, output_ports: outputPorts };
}

if (argument === "--catalog") {
    process.stdout.write(JSON.stringify({ nodes: nodes.map(({ execute, ...definition }) => definition), diagnostics }));
} else if (argument === "--ports") {
    const request = JSON.parse(fs.readFileSync(0, "utf8"));
    const node = nodeByType.get(request.type_id);
    if (!node) throw new Error(`unknown DAG node: ${request.type_id}`);
    process.stdout.write(JSON.stringify(resolvedPorts(node, request.inline_values)));
} else if (argument === "--execute") {
    const request = JSON.parse(fs.readFileSync(0, "utf8"));
    const node = nodeByType.get(request.type_id);
    if (!node) throw new Error(`unknown DAG node: ${request.type_id}`);
    const outputs = await node.execute({
        nodeId: request.node_id,
        nodeName: request.node_name,
        inputs: request.inputs ?? {},
        inline_values: request.inline_values ?? {},
    });
    process.stdout.write(JSON.stringify({ outputs }));
} else if (argument === "--serve") {
    let buffered = "";
    process.stdin.setEncoding("utf8");
    /**
     * Executes one request and writes exactly one protocol response.
     *
     * @param {Record<string, unknown>} request Node execute request.
     * @returns {Promise<void>} Resolves after the response has been written.
     */
    const execute = async (request) => {
        try {
            const node = nodeByType.get(request.type_id);
            if (!node) throw new Error(`unknown DAG node: ${request.type_id}`);
            const outputs = await node.execute(executionContext(request));
            process.stdout.write(`${JSON.stringify({ kind: "execute_response", outputs })}\n`);
        } catch (error) {
            process.stdout.write(`${JSON.stringify({ kind: "execute_response", error: String(error) })}\n`);
        }
    };
    process.stdin.on("data", async (chunk) => {
        buffered += chunk;
        const lines = buffered.split("\n");
        buffered = lines.pop();
        for (const line of lines.filter(Boolean)) {
            try {
                const request = JSON.parse(line);
                if (request.kind === "host_response") {
                    const pending = pendingHostCalls.get(request.id);
                    pendingHostCalls.delete(request.id);
                    if (request.error) pending?.reject(new Error(request.error)); else pending?.resolve(request.result);
                    continue;
                }
                void execute(request);
            } catch (error) {
                process.stdout.write(`${JSON.stringify({ kind: "execute_response", error: String(error) })}\n`);
            }
        }
    });
} else {
    throw new Error("expected --catalog, --ports, --execute, or --serve");
}
