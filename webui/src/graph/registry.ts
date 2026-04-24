// Register all node types from the API with LiteGraph

import { LiteGraph } from "litegraph.js";
import type { NodeTypeInfo, DataTypeMetaData } from "../api/types";
import { getPortColor } from "../ui/theme";
import { normalizeDataType } from "../ui/dialogs/data_types";

/** Module-level registry: type_id → NodeTypeInfo, populated by registerNodeTypes(). */
const nodeTypeRegistry = new Map<string, NodeTypeInfo>();

/** Look up metadata for a node type by its type_id. */
export function getNodeTypeInfo(typeId: string): NodeTypeInfo | undefined {
  return nodeTypeRegistry.get(typeId);
}

/**
 * Register all node types received from /api/registry/types with LiteGraph.
 * Each node type becomes a LiteGraph node class with its ports pre-defined.
 */
export function registerNodeTypes(types: NodeTypeInfo[]): void {
  for (const info of types) {
    nodeTypeRegistry.set(info.type_id, info);
    const inputPorts = info.input_ports;
    const outputPorts = info.output_ports;
    const hasDynIn = info.has_dynamic_input_ports;
    const hasDynOut = info.has_dynamic_output_ports;

    // Build class dynamically
    const NodeClass = class extends (LiteGraph as any).LGraphNode {
      static title = info.display_name;
      static desc = info.description;
      // Store metadata for use by the graph canvas
      static zihuanTypeId = info.type_id;

      constructor() {
        super(info.display_name);
        for (const port of inputPorts) {
          const typeStr = portTypeString(port.data_type);
          this.addInput(port.name, typeStr);
          const last = this.inputs[this.inputs.length - 1];
          const col = getPortColor(typeStr);
          last.color_on  = col;
          last.color_off = port.required ? "#e74c3c" : col;
        }
        for (const port of outputPorts) {
          const typeStr = portTypeString(port.data_type);
          this.addOutput(port.name, typeStr);
          const last = this.outputs[this.outputs.length - 1];
          const col = getPortColor(typeStr);
          last.color_on  = col;
          last.color_off = col;
        }
        // Mark dynamic port nodes so the UI can show add-port buttons
        if (hasDynIn) (this as any).zihuanDynIn = true;
        if (hasDynOut) (this as any).zihuanDynOut = true;
      }
    };

    // Register under the category/type_id path so LiteGraph menus show categories
    const registrationKey = `${info.category}/${info.type_id}`;
    try {
      LiteGraph.registerNodeType(registrationKey, NodeClass as any);
    } catch {
      // Already registered during hot-reload etc.
    }
  }
}

/** Convert a DataTypeMetaData value to a simple litegraph type string. */
export function portTypeString(dt: DataTypeMetaData): string {
  return normalizeDataType(dt);
}
