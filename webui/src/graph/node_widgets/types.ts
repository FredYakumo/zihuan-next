import type { NodeDefinition } from "../../api/types";
import type {
  BrainToolDefinition,
  EmbeddedFunctionConfig,
} from "../../ui/dialogs/index";

export type WidgetMutationCallback = (pending?: Promise<unknown>) => void;

export type EnterSubgraphCallback = (
  parentNodeDef: NodeDefinition,
  mode: "function" | "brain-tool",
  toolIndex?: number,
  toolDef?: BrainToolDefinition,
  functionConfig?: EmbeddedFunctionConfig
) => void;
