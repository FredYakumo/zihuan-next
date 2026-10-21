import { computed, onMounted, ref } from "vue";
import { useRouter } from "vue-router";
import { system, type LlmConfig } from "../../api/client";
import { formatTime, type ModelUsageSummary } from "../model";

export function useModelStatistics() {
    const router = useRouter();
    const models = ref<LlmConfig[]>([]);
    const summaries = ref<ModelUsageSummary[]>([]);
    const query = ref("");
    const stats = computed(() => new Map(summaries.value.map((summary) => [summary.config_id, summary])));
    const filteredModels = computed(() => {
        const keyword = query.value.trim().toLowerCase();
        if (!keyword) return models.value;
        return models.value.filter((model) => {
            const modelName = model.model.type === "chat_llm" ? model.model.llm.model_name : model.model.model_name;
            return [model.name, modelName, model.config_id].some((value) => value.toLowerCase().includes(keyword));
        });
    });
    const loading = ref(false);
    const error = ref("");
    function relativeTime(value?: string | null) {
        if (!value) return "尚未使用";
        const days = Math.floor((Date.now() - new Date(value).getTime()) / 86400000);
        return days < 1 ? "最近" : `${days}天前`;
    }
    async function load() {
        loading.value = true;
        error.value = "";
        try {
            [models.value, summaries.value] = await Promise.all([system.llm.list(), system.modelUsage.list()]);
        } catch (failure: unknown) { error.value = (failure as Error).message; }
        finally { loading.value = false; }
    }
    onMounted(load);

    return { router, models, query, stats, filteredModels, loading, error, relativeTime, load, formatTime };
}
