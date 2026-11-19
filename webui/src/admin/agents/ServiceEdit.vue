<template>
    <section class="page agent-service-page agent-service-edit-page">
        <AdminPageHeader :breadcrumbs="breadcrumbs">
            <t-button variant="outline" @click="triggerServiceImportFile">导入配置</t-button>
            <input ref="serviceImportFileInput" type="file" accept=".json" class="agent-service-import-input"
                @change="handleServiceFileChange" />
            <t-button variant="outline" @click="backToList">返回列表</t-button>
            <t-button v-if="showCreateForm && !form.id" variant="outline" @click="startCreate">返回选择类型</t-button>
            <t-button v-if="showCreateForm || form.id" theme="primary" @click="submitForm">
                {{ form.id ? "保存" : "创建 Service" }}
            </t-button>
        </AdminPageHeader>

        <t-alert v-if="route.query.import === '1'" theme="info" message="请点击右上角的「导入配置」选择 JSON 文件。" />

        <t-loading :loading="initializing" show-overlay>
            <t-card v-if="showCreateForm || form.id" class="agent-service-edit-card" bordered>
                <AgentServiceForm :page="page" />
            </t-card>

            <div v-else class="agent-service-type-grid agent-service-type-grid--page">
                <t-button v-for="type in serviceTypes" :key="type.value" variant="outline"
                    class="agent-service-type-card" @click="pickCreateType(type.value)">
                    <div class="agent-service-type-card-content">
                        <strong>{{ type.label }}</strong>
                        <span class="agent-service-type-hint">{{ type.hint }}</span>
                    </div>
                </t-button>
            </div>
        </t-loading>

        <AgentServiceEditor :page="page" />
    </section>
</template>

<script setup lang="ts">
import { computed, nextTick, onMounted, ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import AdminPageHeader from "../components/AdminPageHeader.vue";
import AgentServiceForm from "../components/AgentServiceForm.vue";
import AgentServiceEditor from "./AgentServiceEditor.vue";
import { useRoleServicePage } from "./roleServicePage";

const route = useRoute();
const router = useRouter();
const page = useRoleServicePage({ autoLoad: false });
const {
    services,
    form,
    showCreateForm,
    serviceTypes,
    load,
    startCreate,
    pickCreateType,
    editService,
    duplicateService,
    submitForm,
    triggerServiceImportFile,
    serviceImportFileInput,
    handleServiceFileChange,
} = page;

const initializing = ref(true);

function queryString(key: string): string {
    const value = route.query[key];
    return typeof value === "string" ? value : "";
}

const breadcrumbs = computed(() => [
    { label: "Service 管理", to: "/services" },
    { label: form.id ? "编辑 Service" : "新建 Service" },
]);

function backToList() {
    void router.push("/services");
}

onMounted(async () => {
    try {
        await load();
        const id = queryString("id");
        const copy = queryString("copy");
        if (id) {
            const service = services.value.find((entry) => entry.config_id === id);
            if (!service) {
                alert("Service 不存在或已被删除");
                await router.replace("/services");
                return;
            }
            editService(service);
        } else if (copy) {
            const service = services.value.find((entry) => entry.config_id === copy);
            if (!service) {
                alert("Service 不存在或已被删除");
                await router.replace("/services");
                return;
            }
            duplicateService(service);
        } else if (route.query.import === "1") {
            await nextTick();
            triggerServiceImportFile();
        } else {
            startCreate();
        }
    } catch (error) {
        alert(`Service 加载失败: ${(error as Error).message}`);
    } finally {
        initializing.value = false;
    }
});
</script>

<style scoped lang="scss">
@use "./role-service" as *;
</style>
