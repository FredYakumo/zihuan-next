// Node widget setup adds inline value widgets and special editor buttons to LiteGraph nodes.

import type { NodeDefinition } from "../api/types";
import { setupBrainWidgets } from "./node_widgets/brain";
import { setupFormatStringWidgets } from "./node_widgets/format_string";
import { setupFunctionWidgets } from "./node_widgets/function_node";
import { setupJsonExtractWidgets } from "./node_widgets/json_extract";
import { setupLLMMessageListWidgets } from "./node_widgets/llm_message_list_data";
import { setupQQMessageListWidgets } from "./node_widgets/qq_message_list_data";
import { setupQQMessagePreviewWidgets } from "./node_widgets/qq_message_preview";
import { setupStringDataWidgets } from "./node_widgets/string_data";
import type {
  EnterSubgraphCallback,
  WidgetMutationCallback,
} from "./node_widgets/types";
import { setupConfigFieldWidgets } from "./widget_system/config_field_widgets";
import { setupSimpleInlineWidgets } from "./widget_system/inline_widgets";

/** Called for every node added to the canvas after the node is created. */
export function setupNodeWidgets(
  lNode: any,
  nodeDef: NodeDefinition,
  getSessionId: () => string | null,
  onRefresh: () => void,
  onEnterSubgraph: EnterSubgraphCallback,
  onMutated?: WidgetMutationCallback
): void {
  const typeId = nodeDef.node_type;

  switch (typeId) {
    case "format_string":
      setupFormatStringWidgets(lNode, nodeDef, getSessionId, onRefresh);
      break;
    case "json_extract":
      setupJsonExtractWidgets(lNode, nodeDef, getSessionId, onRefresh);
      break;
    case "function":
      setupFunctionWidgets(lNode, nodeDef, getSessionId, onRefresh, onEnterSubgraph);
      break;
    case "brain":
    case "qq_chat":
      setupBrainWidgets(lNode, nodeDef, getSessionId, onRefresh, onEnterSubgraph);
      break;
    case "string_data":
      setupStringDataWidgets(lNode, nodeDef, getSessionId, onMutated);
      break;
    case "message_list_data":
      setupLLMMessageListWidgets(lNode, nodeDef, getSessionId, onRefresh);
      break;
    case "qq_message_list_data":
      setupQQMessageListWidgets(lNode, nodeDef, getSessionId, onRefresh);
      break;
    case "qq_message_preview":
      setupQQMessagePreviewWidgets(lNode, nodeDef);
      break;
    default:
      setupSimpleInlineWidgets(lNode, nodeDef, getSessionId, onMutated);
      break;
  }

  setupConfigFieldWidgets(lNode, nodeDef, getSessionId, onMutated);
}
