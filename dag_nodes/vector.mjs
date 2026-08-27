import { port } from "../graph_engine/zihuan_sdk.mjs";

function cosine(left, right) {
  if (!Array.isArray(left) || !Array.isArray(right) || left.length === 0 || right.length === 0 || left.length !== right.length) {
    throw new Error("vectors must be non-empty and have the same dimension");
  }
  let dot = 0;
  let leftNorm = 0;
  let rightNorm = 0;
  for (let index = 0; index < left.length; index += 1) {
    const leftValue = Number(left[index]);
    const rightValue = Number(right[index]);
    if (!Number.isFinite(leftValue) || !Number.isFinite(rightValue)) throw new Error("vectors must contain finite numbers");
    dot += leftValue * rightValue;
    leftNorm += leftValue * leftValue;
    rightNorm += rightValue * rightValue;
  }
  if (leftNorm === 0 || rightNorm === 0) throw new Error("vectors must not have zero norm");
  return dot / Math.sqrt(leftNorm * rightNorm);
}

/** @type {import("../graph_engine/zihuan_sdk.mjs").NodeDefinition[]} */
export const nodes = [
  {
    type_id: "vector_cosine_similarity", display_name: "向量余弦相似度", category: "AI", description: "计算两个向量的余弦相似度",
    input_ports: [port("left", "Vector"), port("right", "Vector")], output_ports: [port("similarity", "Float")],
    execute: ({ inputs }) => ({ similarity: cosine(inputs.left, inputs.right) }),
  },
  {
    type_id: "top_k_similarity", display_name: "Top-K相似检索", category: "AI", description: "对 Vec<Vector> 与查询向量执行 top-k 相似度检索",
    input_ports: [port("vectors", { Vec: "Vector" }), port("query", "Vector"), port("top_k", "Integer")], output_ports: [port("indices", { Vec: "Integer" }), port("scores", { Vec: "Float" }), port("vectors", { Vec: "Vector" })],
    execute: ({ inputs }) => {
      if (!Array.isArray(inputs.vectors) || inputs.vectors.length === 0) throw new Error("vectors input must not be empty");
      const topK = Number(inputs.top_k);
      if (!Number.isInteger(topK) || topK <= 0) throw new Error("top_k must be greater than 0");
      const matches = inputs.vectors.map((vector, index) => ({ index, vector, score: cosine(vector, inputs.query) }))
        .sort((left, right) => right.score - left.score || left.index - right.index)
        .slice(0, topK);
      return { indices: matches.map((match) => match.index), scores: matches.map((match) => match.score), vectors: matches.map((match) => match.vector) };
    },
  },
];
