<template>
  <CommonSelect v-model="selection" :options="options" filterable :clearable="clearable" :disabled="disabled"
    :placeholder="placeholder" @change="select">
    <t-option class="common-select-create-option" value="__create_database__" label="新增关系型数据库">
      <span class="common-select-create-option-content"><AddIcon />新增关系型数据库</span>
    </t-option>
  </CommonSelect>
  <p v-if="error && !creating" role="alert">{{ error }}</p>
  <t-dialog v-model:visible="creating" header="新增关系型数据库" :confirm-btn="{ content: '保存', loading: saving }"
    :close-on-overlay-click="false" @confirm="create">
    <t-form label-width="100px">
      <t-form-item label="名称"><t-input v-model="name" /></t-form-item>
      <t-form-item label="数据库类型">
        <t-select v-model="type"><t-option value="sqlite" label="SQLite" /><t-option value="mysql" label="MySQL" /></t-select>
      </t-form-item>
      <t-form-item :label="type === 'sqlite' ? '数据库文件路径' : '连接 URL'">
        <t-input v-model="address" :type="type === 'mysql' ? 'password' : 'text'"
          :placeholder="type === 'sqlite' ? '数据库文件的完整路径' : 'mysql://user:password@host:3306/database'" />
      </t-form-item>
      <p v-if="error" role="alert">{{ error }}</p>
    </t-form>
  </t-dialog>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { AddIcon } from "tdesign-icons-vue-next";
import { system, type ConnectionConfig } from "../../api/client";
import CommonSelect from "./CommonSelect.vue";

const props = withDefaults(defineProps<{
  modelValue: string;
  clearable?: boolean;
  disabled?: boolean;
  placeholder?: string;
}>(), { clearable: false, disabled: false, placeholder: "请选择关系型数据库" });
const emit = defineEmits<{ "update:modelValue": [value: string] }>();
const selection = ref(props.modelValue);
const connections = ref<ConnectionConfig[]>([]);
const creating = ref(false);
const saving = ref(false);
const name = ref("");
const type = ref<"sqlite" | "mysql">("sqlite");
const address = ref("");
const error = ref("");
const options = computed(() => connections.value
  .filter((connection) => connection.kind.type === "mysql" || connection.kind.type === "sqlite")
  .map((connection) => {
    const defaultSqlite = connection.kind.type === "sqlite"
      && ["系统配置数据库（默认 SQLite）", "sqlite3数据库"].includes(connection.name);
    return {
      value: connection.config_id,
      label: defaultSqlite ? "sqlite3数据库" : connection.name,
      description: defaultSqlite ? undefined : connection.kind.type,
    };
  }));

watch(() => props.modelValue, (value) => { selection.value = value; });
watch(type, () => { address.value = ""; });

async function reload() {
  try { connections.value = await system.connections.list(); }
  catch (cause) { error.value = String(cause); }
}

function select(value: string | undefined) {
  if (value === "__create_database__") {
    selection.value = props.modelValue;
    error.value = "";
    creating.value = true;
  } else {
    selection.value = props.modelValue;
    emit("update:modelValue", value ?? "");
  }
}

async function create() {
  if (!name.value.trim() || !address.value.trim()) {
    error.value = "请填写名称和数据库连接信息";
    return;
  }
  saving.value = true;
  error.value = "";
  try {
    const kind = type.value === "sqlite"
      ? { type: "sqlite" as const, path: address.value.trim() }
      : { type: "mysql" as const, url: address.value.trim(), max_connections: 32, acquire_timeout_secs: 30 };
    const connection = await system.connections.create({ name: name.value.trim(), enabled: true, kind });
    await reload();
    creating.value = false;
    selection.value = props.modelValue;
    emit("update:modelValue", connection.config_id);
  } catch (cause) { error.value = String(cause); }
  finally { saving.value = false; }
}

onMounted(reload);
</script>
