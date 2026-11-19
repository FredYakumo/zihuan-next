<template>
  <t-form class="connection-form" label-align="top">
    <div class="connection-form-section">
      <h3>基本信息</h3>
      <div class="connection-form-grid">
        <t-form-item label="名称" required>
          <t-input v-model="form.name" placeholder="请输入连接名称" />
        </t-form-item>
        <t-form-item label="连接类型" required>
          <t-select v-model="form.type" :disabled="!isCreating">
            <t-option v-for="type in connectionTypes" :key="type.value" :value="type.value" :label="type.label" />
          </t-select>
        </t-form-item>
      </div>
      <t-checkbox v-model="form.enabled">启用该连接</t-checkbox>
    </div>

    <div class="connection-form-section">
      <h3>{{ connectionTypeLabel(form.type) }} 配置</h3>
      <div class="connection-form-grid">
        <template v-if="form.type === 'mysql'">
          <t-form-item label="地址" required><t-input v-model="form.mysql_host" placeholder="127.0.0.1" /></t-form-item>
          <t-form-item label="端口" required><t-input v-model="form.mysql_port" placeholder="3306" /></t-form-item>
          <t-form-item label="账号（可选）"><t-input v-model="form.mysql_user" /></t-form-item>
          <t-form-item label="密码（可选）"><ConnectionCredentialInput v-model="form.mysql_password" placeholder="请输入密码" /></t-form-item>
          <t-form-item class="connection-form-item--full" label="数据库名" required><t-input v-model="form.mysql_database" placeholder="zihuan" /></t-form-item>
          <t-form-item label="最大连接数"><t-input-number v-model="form.mysql_max_connections" :min="1" /></t-form-item>
          <t-form-item label="获取连接超时（秒）"><t-input-number v-model="form.mysql_acquire_timeout_secs" :min="1" /></t-form-item>
        </template>

        <template v-else-if="form.type === 'redis'">
          <t-form-item class="connection-form-item--full" label="URL"><t-input v-model="form.redis_url" placeholder="redis://127.0.0.1:6379" /></t-form-item>
          <t-form-item label="用户名（可选）"><t-input v-model="form.redis_username" placeholder="default" /></t-form-item>
          <t-form-item label="密码（可选）"><ConnectionCredentialInput v-model="form.redis_password" /></t-form-item>
        </template>

        <template v-else-if="form.type === 'weaviate'">
          <t-form-item label="Base URL" required><t-input v-model="form.weaviate_base_url" /></t-form-item>
          <t-form-item label="认证方式"><t-select v-model="form.weaviate_auth_method" @change="clearInactiveConnectionCredentials('weaviate')"><t-option value="password" label="密码" /><t-option value="api_key" label="API Key" /></t-select></t-form-item>
          <t-form-item v-if="form.weaviate_auth_method === 'password'" label="用户名" required><t-input v-model="form.weaviate_username" /></t-form-item>
          <t-form-item :label="form.weaviate_auth_method === 'password' ? '密码' : 'API Key'" required><ConnectionCredentialInput v-if="form.weaviate_auth_method === 'password'" v-model="form.weaviate_password" /><ConnectionCredentialInput v-else v-model="form.weaviate_api_key" /></t-form-item>
        </template>

        <template v-else-if="form.type === 'elasticsearch'">
          <t-form-item label="Base URL" required><t-input v-model="form.elasticsearch_base_url" placeholder="https://localhost:9200" /></t-form-item>
          <t-form-item label="认证方式"><t-select v-model="form.elasticsearch_auth_method" @change="clearInactiveConnectionCredentials('elasticsearch')"><t-option value="password" label="密码" /><t-option value="api_key" label="API Key" /></t-select></t-form-item>
          <t-form-item v-if="form.elasticsearch_auth_method === 'password'" label="用户名" required><t-input v-model="form.elasticsearch_username" /></t-form-item>
          <t-form-item :label="form.elasticsearch_auth_method === 'password' ? '密码' : 'API Key'" required><ConnectionCredentialInput v-if="form.elasticsearch_auth_method === 'password'" v-model="form.elasticsearch_password" /><ConnectionCredentialInput v-else v-model="form.elasticsearch_api_key" /></t-form-item>
          <t-form-item label="向量维度"><t-input-number v-model="form.elasticsearch_vector_dimensions" :min="1" /></t-form-item>
        </template>

        <template v-else-if="form.type === 'rustfs'">
          <t-form-item label="Endpoint"><t-input v-model="form.rustfs_endpoint" /></t-form-item>
          <t-form-item label="Bucket"><t-input v-model="form.rustfs_bucket" /></t-form-item>
          <t-form-item label="Region"><t-input v-model="form.rustfs_region" /></t-form-item>
          <t-form-item label="Access Key"><t-input v-model="form.rustfs_access_key" /></t-form-item>
          <t-form-item label="Secret Key"><ConnectionCredentialInput v-model="form.rustfs_secret_key" /></t-form-item>
          <t-form-item label="Public Base URL"><t-input v-model="form.rustfs_public_base_url" /></t-form-item>
          <t-form-item class="connection-form-item--full"><t-checkbox v-model="form.rustfs_path_style">使用 path-style</t-checkbox></t-form-item>
        </template>

        <template v-else-if="isBotAdapterConnectionType(form.type)">
          <t-form-item class="connection-form-item--full" label="渠道">
            <div class="channel-picker" role="radiogroup" aria-label="Bot 渠道">
              <button
                type="button"
                role="radio"
                :aria-checked="!form.wechat_enabled"
                :class="['channel-card', { 'channel-card--active': !form.wechat_enabled }]"
                @click="form.wechat_enabled = false"
              >
                <img class="channel-card-icon" :src="qqIcon" alt="" />
                <span class="channel-card-text"><strong>QQ</strong><small>OneBot</small></span>
              </button>
              <button
                type="button"
                role="radio"
                :aria-checked="form.wechat_enabled"
                :class="['channel-card', { 'channel-card--active': form.wechat_enabled }]"
                @click="selectWechatChannel"
              >
                <img class="channel-card-icon" :src="wechatIcon" alt="" />
                <span class="channel-card-text"><strong>微信</strong><small>WeChatPadPro</small></span>
              </button>
            </div>
          </t-form-item>
          <template v-if="form.wechat_enabled">
            <t-form-item label="Pad URL"><t-input v-model="form.wechat_pad_url" placeholder="http://127.0.0.1:8849" /></t-form-item>
            <t-form-item label="Pad adminKey"><ConnectionCredentialInput v-model="form.wechat_pad_token" /></t-form-item>
            <t-form-item label="wxid"><t-input v-model="form.wechat_self_wxid" placeholder="" /></t-form-item>
            <t-form-item v-if="!form.wechat_relay_url" label="Webhook Token"><t-input v-model="form.wechat_webhook_token" placeholder="回调路径里的随机串" /></t-form-item>
            <t-form-item label="昵称"><t-input v-model="form.wechat_nickname" /></t-form-item>
          </template>
          <template v-else>
            <t-form-item label="Bot WS URL"><t-input v-model="form.bot_server_url" placeholder="ws://192.168.71.2:3008" /></t-form-item>
            <t-form-item label="Adapter HTTP URL"><t-input v-model="form.adapter_server_url" placeholder="http://192.168.71.2:3001" /></t-form-item>
            <t-form-item label="QQ 号"><t-input v-model="form.qq_id" /></t-form-item>
            <t-form-item label="Token"><ConnectionCredentialInput v-model="form.bot_server_token" /></t-form-item>
          </template>
        </template>

        <template v-else-if="form.type === 'web_search_engine'">
          <t-form-item label="Provider">
            <t-select v-model="form.web_search_engine_provider">
              <template #valueDisplay>
                <span class="provider-option">
                  {{ providerDisplay(form.web_search_engine_provider).label }}
                  <span v-if="providerDisplay(form.web_search_engine_provider).plugin" class="provider-plugin-badge">Plugin</span>
                </span>
              </template>
              <t-option v-for="option in webSearchProviderOptions" :key="option.value" :value="option.value" :label="option.label">
                <span class="provider-option">{{ option.label }}<span v-if="option.plugin" class="provider-plugin-badge">Plugin</span></span>
              </t-option>
            </t-select>
          </t-form-item>
          <t-form-item label="Timeout（秒）"><t-input-number v-model="form.web_search_engine_timeout_secs" :min="1" /></t-form-item>
          <t-form-item
            v-for="field in selectedWebSearchConfigFields"
            :key="field.name"
            :required="!field.optional"
            class="connection-form-item--full"
          >
            <template #label>
              <span class="web-search-field-label">
                {{ field.label + (field.optional ? '（可选）' : '') }}
                <t-tooltip v-if="field.description" :content="field.description" trigger="click">
                  <button type="button" class="web-search-field-help" :aria-label="`${field.label}说明`" @click.stop.prevent>?</button>
                </t-tooltip>
              </span>
            </template>
            <ConnectionCredentialInput v-if="field.secret || field.name === 'api_token'" :model-value="webSearchFieldValue(field.name)" @update:model-value="(v: string) => webSearchFieldValue(field.name, v)" />
            <t-input v-else :model-value="webSearchFieldValue(field.name)" @update:model-value="(v: string) => webSearchFieldValue(field.name, v)" />
          </t-form-item>
        </template>

        <template v-else-if="form.type === 'tokenizer'">
          <t-form-item class="connection-form-item--full" label="Tokenizer 模型" required><t-select v-model="form.tokenizer_model_name" placeholder="请选择"><t-option v-for="model in tokenizerModels" :key="model" :value="model" :label="model" /></t-select></t-form-item>
        </template>

        <template v-else-if="form.type === 'sqlite'">
          <t-form-item class="connection-form-item--full" label="数据库文件路径"><t-input v-model="form.sqlite_path" placeholder="/path/to/database.db" /></t-form-item>
        </template>
      </div>
    </div>
  </t-form>
</template>

<script lang="ts">
import { defineComponent, type PropType } from "vue";
import ConnectionCredentialInput from "../components/ConnectionCredentialInput.vue";
import qqIcon from "../../assets/icons/qq.svg";
import wechatIcon from "../../assets/icons/wechat.svg";
import type { UseConnectionsReturn } from "./useConnections";

/** The callback token only has to be unguessable; the URL itself is derived by the backend. */
function randomWebhookToken() {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

export default defineComponent({
  name: "ConnectionForm",
  components: { ConnectionCredentialInput },
  props: {
    page: { type: Object as PropType<UseConnectionsReturn>, required: true },
  },
  setup(props) {
    const page = props.page;
    function selectWechatChannel() {
      page.form.wechat_enabled = true;
      if (!page.form.wechat_webhook_token.trim()) {
        page.form.wechat_webhook_token = randomWebhookToken();
      }
    }
    return {
      ...page,
      qqIcon,
      wechatIcon,
      selectWechatChannel,
    };
  },
});
</script>

<style scoped lang="scss">
@use "./connections" as *;
</style>
