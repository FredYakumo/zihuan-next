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

async function loadNodes() {
  const catalog = [];
  for (const file of await nodeModuleFiles(nodeDirectory)) {
    let module;
    try {
      module = await import(pathToFileURL(file).href);
    } catch (error) {
      throw new Error(`failed to load DAG node module ${path.relative(nodeDirectory, file)}: ${error.message}`);
    }
    if (module.nodes === undefined) continue;
    if (!Array.isArray(module.nodes)) {
      throw new Error(`DAG node module ${path.relative(nodeDirectory, file)} must export a nodes array`);
    }
    catalog.push(...module.nodes);
  }
  const seen = new Set();
  for (const node of catalog) {
    if (!node || typeof node.type_id !== "string" || node.type_id.trim() === "" || typeof node.execute !== "function") {
      throw new Error("DAG node catalog contains invalid node metadata");
    }
    if (seen.has(node.type_id)) throw new Error(`duplicate DAG node type_id: ${node.type_id}`);
    seen.add(node.type_id);
  }
  return catalog;
}

function hostCall(method, params = {}) {
  const id = ++requestId;
  process.stdout.write(`${JSON.stringify({ kind: "host_request", id, method, params })}\n`);
  return new Promise((resolve, reject) => pendingHostCalls.set(id, { resolve, reject }));
}

function executionContext(request) {
  return {
    nodeId: request.node_id,
    nodeName: request.node_name,
    inputs: hydrateResources(request.inputs ?? {}),
    inline_values: request.inline_values ?? {},
    sdk: createZihuanSdk(hostCall),
  };
}

const nodes = await loadNodes();
const nodeByType = new Map(nodes.map((node) => [node.type_id, node]));

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
  process.stdout.write(JSON.stringify(nodes.map(({ execute, ...definition }) => definition)));
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
