import type { NodeDefinition } from "../../api/types";
import type {
  ToolDefinition,
  EmbeddedFunctionConfig,
} from "../../ui/dialogs/index";

export type WidgetMutationCallback = (pending?: Promise<unknown>) => void;

export type EnterSubgraphCallback = (
  parentNodeDef: NodeDefinition,
  mode: "function" | "tool-calling-tool",
  toolIndex?: number,
  toolDef?: ToolDefinition,
  functionConfig?: EmbeddedFunctionConfig
) => void;
