import type { NodeDefinition } from "../../api/types";
import {
  openFunctionSignatureEditor,
  type EmbeddedFunctionConfig,
} from "../../ui/dialogs/index";
import type { EnterSubgraphCallback } from "./types";

export function setupFunctionWidgets(
  lNode: any,
  nodeDef: NodeDefinition,
  getSessionId: () => string | null,
  onRefresh: () => void,
  onEnterSubgraph: EnterSubgraphCallback
): void {
  const currentFunctionConfig = (): EmbeddedFunctionConfig => ({
    name: ((nodeDef.inline_values?.["function_config"] as EmbeddedFunctionConfig | undefined)?.name) ?? nodeDef.name,
    description: ((nodeDef.inline_values?.["function_config"] as EmbeddedFunctionConfig | undefined)?.description) ?? "",
    inputs: ((nodeDef.inline_values?.["function_config"] as EmbeddedFunctionConfig | undefined)?.inputs) ?? [],
    outputs: ((nodeDef.inline_values?.["function_config"] as EmbeddedFunctionConfig | undefined)?.outputs) ?? [],
    subgraph: ((nodeDef.inline_values?.["function_config"] as EmbeddedFunctionConfig | undefined)?.subgraph) ?? {
      nodes: [],
      edges: [],
      graph_inputs: [],
      graph_outputs: [],
      hyperparameter_groups: [],
      hyperparameters: [],
      variables: [],
    } as any,
  });

  lNode.addWidget("button", "编辑签名/子图", null, () => {
    const sid = getSessionId();
    if (!sid) {
      alert("请先打开一个图。");
      return;
    }
    openFunctionSignatureEditor(
      nodeDef,
      sid,
      onRefresh,
      (config) => onEnterSubgraph(nodeDef, "function", undefined, undefined, config)
    );
  });

  lNode.addWidget("button", "进入子图", null, () => {
    const sid = getSessionId();
    if (!sid) {
      alert("请先打开一个图。");
      return;
    }
    onEnterSubgraph(nodeDef, "function", undefined, undefined, currentFunctionConfig());
  });
}
