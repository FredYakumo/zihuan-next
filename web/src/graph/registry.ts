// Register all node types from the API with LiteGraph

import { LiteGraph } from "@comfyorg/litegraph";
import type { NodeTypeInfo } from "../api/types";

/**
 * Register all node types received from /api/registry/types with LiteGraph.
 * Each node type becomes a LiteGraph node class with its ports pre-defined.
 */
export function registerNodeTypes(types: NodeTypeInfo[]): void {
  for (const info of types) {
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
          this.addInput(port.name, portTypeString(port.data_type as string | object));
        }
        for (const port of outputPorts) {
          this.addOutput(port.name, portTypeString(port.data_type as string | object));
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

/** Convert a DataType (possibly nested) to a simple litegraph type string. */
export function portTypeString(dt: string | object): string {
  if (typeof dt === "string") return dt;
  // Handle Vec / other wrapper types
  const keys = Object.keys(dt as object);
  if (keys.length > 0) return `${keys[0]}<${Object.values(dt as object)[0] as string}>`;
  return "*";
}
