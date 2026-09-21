<template>
  <t-drawer
    v-model:visible="visible"
    header="Rate Limit"
    size="820px"
    :close-on-overlay-click="false"
  >
    <div class="rate-limit-hint">调用频率限制，优先级：用户 &gt; 群组 &gt; 默认。窗口可按 N 分钟 / N 小时 / N 天，计数跟随用户（跨群与私聊共享）。</div>

    <t-card class="rate-limit-section" :bordered="false">
      <template #title>
        <div class="rate-limit-title-row">
          <span>默认规则</span>
          <t-checkbox v-model="form.message_rate_limit_default_enabled">启用</t-checkbox>
        </div>
      </template>
      <div v-if="form.message_rate_limit_default_enabled" class="rate-limit-grid">
        <t-form-item label="模式">
          <t-select v-model="form.message_rate_limit_default.unlimited">
            <t-option :value="false" label="限次" />
            <t-option :value="true" label="无限" />
          </t-select>
        </t-form-item>
        <template v-if="!form.message_rate_limit_default.unlimited">
          <t-form-item label="窗口">
            <div class="rate-limit-window">
              <t-input-number v-model="form.message_rate_limit_default.window_size" :min="1" />
              <t-select v-model="form.message_rate_limit_default.window_unit">
                <t-option value="minute" label="分钟" />
                <t-option value="hour" label="小时" />
                <t-option value="day" label="天" />
              </t-select>
            </div>
          </t-form-item>
          <t-form-item label="次数"><t-input-number v-model="form.message_rate_limit_default.max_calls" :min="1" /></t-form-item>
        </template>
      </div>
    </t-card>

    <t-card class="rate-limit-section" :bordered="false">
      <template #title>
        <div class="rate-limit-title-row">
          <span>群组规则</span>
          <t-button variant="text" @click="addGroupRule">新增群组规则</t-button>
        </div>
      </template>
      <div v-if="form.message_rate_limit_groups.length === 0" class="rate-limit-empty">还没有群组规则。</div>
      <t-card v-for="(rule, index) in form.message_rate_limit_groups" :key="`group-${index}`" :bordered="true" class="rate-limit-rule-card">
        <template #title>
          <div class="rate-limit-rule-header">
            <strong>群组规则 {{ index + 1 }}</strong>
            <t-button variant="text" theme="danger" size="small" @click="form.message_rate_limit_groups.splice(index, 1)">移除</t-button>
          </div>
        </template>
        <div class="rate-limit-grid">
          <t-form-item label="Group ID"><t-input v-model="rule.group_id" /></t-form-item>
          <t-form-item label="模式">
            <t-select v-model="rule.unlimited">
              <t-option :value="false" label="限次" />
              <t-option :value="true" label="无限" />
            </t-select>
          </t-form-item>
          <template v-if="!rule.unlimited">
            <t-form-item label="窗口">
              <div class="rate-limit-window">
                <t-input-number v-model="rule.window_size" :min="1" />
                <t-select v-model="rule.window_unit">
                  <t-option value="minute" label="分钟" />
                  <t-option value="hour" label="小时" />
                  <t-option value="day" label="天" />
                </t-select>
              </div>
            </t-form-item>
            <t-form-item label="次数"><t-input-number v-model="rule.max_calls" :min="1" /></t-form-item>
          </template>
        </div>
      </t-card>
    </t-card>

    <t-card class="rate-limit-section" :bordered="false">
      <template #title>
        <div class="rate-limit-title-row">
          <span>用户规则</span>
          <t-button variant="text" @click="addUserRule">新增用户规则</t-button>
        </div>
      </template>
      <div v-if="form.message_rate_limit_users.length === 0" class="rate-limit-empty">还没有用户规则。</div>
      <t-card v-for="(rule, index) in form.message_rate_limit_users" :key="`user-${index}`" :bordered="true" class="rate-limit-rule-card">
        <template #title>
          <div class="rate-limit-rule-header">
            <strong>用户规则 {{ index + 1 }}</strong>
            <t-button variant="text" theme="danger" size="small" @click="form.message_rate_limit_users.splice(index, 1)">移除</t-button>
          </div>
        </template>
        <div class="rate-limit-grid">
          <t-form-item label="Sender ID"><t-input v-model="rule.sender_id" /></t-form-item>
          <t-form-item label="模式">
            <t-select v-model="rule.unlimited">
              <t-option :value="false" label="限次" />
              <t-option :value="true" label="无限" />
            </t-select>
          </t-form-item>
          <template v-if="!rule.unlimited">
            <t-form-item label="窗口">
              <div class="rate-limit-window">
                <t-input-number v-model="rule.window_size" :min="1" />
                <t-select v-model="rule.window_unit">
                  <t-option value="minute" label="分钟" />
                  <t-option value="hour" label="小时" />
                  <t-option value="day" label="天" />
                </t-select>
              </div>
            </t-form-item>
            <t-form-item label="次数"><t-input-number v-model="rule.max_calls" :min="1" /></t-form-item>
          </template>
        </div>
      </t-card>
    </t-card>

    <template #footer>
      <div class="rate-limit-footer"><t-button theme="primary" @click="visible = false">完成</t-button></div>
    </template>
  </t-drawer>
</template>

<script setup lang="ts">
import { defaultQqChatMessageRateLimitRule, type ServiceFormState } from "../model";

const visible = defineModel<boolean>("visible", { required: true });
const props = defineProps<{ form: ServiceFormState }>();

function addGroupRule() {
  props.form.message_rate_limit_groups.push({ group_id: "", ...defaultQqChatMessageRateLimitRule() });
}

function addUserRule() {
  props.form.message_rate_limit_users.push({ sender_id: "", ...defaultQqChatMessageRateLimitRule() });
}
</script>

<style scoped lang="scss">
.rate-limit-hint {
  margin-top: 4px;
  color: var(--td-text-color-placeholder);
  font-size: 12px;
  line-height: 1.5;
}

.rate-limit-section {
  margin-top: 12px;
  margin-bottom: 16px;
}

.rate-limit-section :deep(.t-card__header-wrapper > div:not([class])) {
  flex: 1;
  min-width: 0;
}

.rate-limit-section :deep(.t-card__title) {
  font-size: 15px;
  font-weight: 600;
}

.rate-limit-title-row,
.rate-limit-rule-header,
.rate-limit-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  width: 100%;
}

.rate-limit-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 12px 24px;
}

.rate-limit-grid :deep(.t-form__item) {
  margin-bottom: 0;
}

.rate-limit-window {
  display: flex;
  gap: 6px;
}

.rate-limit-window :deep(.t-input-number),
.rate-limit-window :deep(.t-select) {
  width: 100px;
}

.rate-limit-rule-card {
  margin-top: 12px;
}

.rate-limit-empty {
  padding: 24px 0;
  color: var(--td-text-color-placeholder);
  text-align: center;
}

.rate-limit-footer {
  justify-content: flex-end;
}

@media (max-width: 840px) {
  .rate-limit-grid {
    grid-template-columns: 1fr;
  }
}
</style>
