<template>
    <div class="avatar-editor">
        <input ref="fileInput" type="file" accept="image/*" style="display: none" @change="handleFileSelect" />
        <button type="button" class="avatar-editor-trigger" :title="avatarUrl ? '更换头像' : '上传头像'"
            @click="openEditor">
            <img v-if="displayUrl" :src="displayUrl" alt="Avatar preview" class="avatar-editor-preview" />
            <div v-else class="avatar-editor-placeholder">{{ initial }}</div>
            <div class="avatar-editor-overlay">
                <EditIcon />
            </div>
        </button>
        <t-dialog v-model:visible="editorVisible" header="编辑头像" width="420px" :close-on-overlay-click="false"
            :confirm-btn="{ content: '使用此头像', loading: uploading }" cancel-btn="取消" @confirm="confirmCrop"
            @cancel="closeEditor" @close="closeEditor">
            <div class="avatar-editor-dialog">
                <div class="avatar-editor-preview-row">
                    <div class="avatar-editor-crop-column">
                        <t-button variant="outline" shape="circle" class="avatar-editor-rotate-handle"
                            :disabled="!draftUrl" aria-label="进入自由旋转模式" title="按住并拖动以旋转头像"
                            @pointerdown="startRotation">
                            <RotateIcon />
                        </t-button>
                        <div :key="draftUrl || 'avatar-placeholder'" class="avatar-editor-crop-window"
                            @pointerdown.capture="startCrop" @pointermove="moveCrop" @pointerup="endCrop"
                            @pointercancel="endCrop" @lostpointercapture="endCrop" @dragstart.prevent>
                            <img v-if="draftUrl" :key="draftUrl" :src="draftUrl" alt="头像裁剪预览" :draggable="false"
                                class="avatar-editor-crop-image"
                                :style="{ transform: `translate(${pan.x}px, ${pan.y}px) scale(${zoom}) rotate(${rotation}deg)` }" />
                            <div v-else class="avatar-editor-placeholder">{{ initial }}</div>
                        </div>
                    </div>
                    <div v-if="draftUrl" class="avatar-editor-vertical-zoom">
                        <div class="avatar-editor-zoom-ruler" aria-hidden="true">
                            <span v-for="tick in zoomRulerTicks" :key="tick" class="avatar-editor-zoom-tick"
                                :class="{ 'avatar-editor-zoom-tick--major': tick % 1 === 0 }" />
                        </div>
                        <t-slider v-model="zoom" :min="1" :max="3" :step="0.05" layout="vertical" />
                    </div>
                </div>
                <div class="avatar-editor-actions">
                    <t-button variant="outline" @click="fileInput?.click()">选择图片</t-button>
                    <t-button variant="text" shape="square" :disabled="!draftUrl" title="还原图片默认"
                        aria-label="还原图片默认" @click="resetTransform">
                        <RefreshIcon />
                    </t-button>
                    <t-button variant="text" theme="danger" @click="clearDraft">使用默认头像</t-button>
                </div>
                <div class="avatar-editor-hint">拖动头像可移动位置；按住头像上方的旋转图标并拖动可自由旋转。</div>
            </div>
        </t-dialog>
    </div>
</template>
<script setup lang="ts">
import { computed, reactive, ref } from "vue";
import { EditIcon, RefreshIcon, RotateIcon } from "tdesign-icons-vue-next";
import { getAvatarDisplayUrl } from "../model";

const props = defineProps<{
    /** Persisted avatar reference: an external URL or `avatar://<id>`; empty means the default placeholder. */
    avatarUrl?: string | null;
    /** Entity name used to derive the placeholder initial. */
    name?: string;
}>();
const emit = defineEmits<{ (event: "update:avatarUrl", value: string): void }>();

const ZOOM_RULER_TICKS = [3, 2.75, 2.5, 2.25, 2, 1.75, 1.5, 1.25, 1];
const CROP_WINDOW_SIZE = 280;

const fileInput = ref<HTMLInputElement | null>(null);
const editorVisible = ref(false);
const uploading = ref(false);
const draftUrl = ref("");
const draftFile = ref<File | null>(null);
const zoom = ref(1);
const rotation = ref(0);
const pan = reactive({ x: 0, y: 0 });
const panStart = reactive({ x: 0, y: 0, pointerX: 0, pointerY: 0 });
const cropping = ref(false);
const rotating = ref(false);
const rotationStart = reactive({ x: 0, value: 0 });
const zoomRulerTicks = ZOOM_RULER_TICKS;

const displayUrl = computed(() => getAvatarDisplayUrl(String(props.avatarUrl ?? "")));
const initial = computed(() => (props.name ? props.name.slice(0, 1).toUpperCase() : "A"));

function openEditor() {
    editorVisible.value = true;
    draftUrl.value = displayUrl.value;
    draftFile.value = null;
    zoom.value = 1;
    rotation.value = 0;
    Object.assign(pan, { x: 0, y: 0 });
}

function closeEditor() {
    editorVisible.value = false;
    draftFile.value = null;
    if (draftUrl.value.startsWith("blob:")) URL.revokeObjectURL(draftUrl.value);
    draftUrl.value = "";
}

function resetTransform() {
    zoom.value = 1;
    rotation.value = 0;
    Object.assign(pan, { x: 0, y: 0 });
}

function startRotation(event: PointerEvent) {
    if (!draftUrl.value) return;
    event.preventDefault();
    rotating.value = true;
    rotationStart.x = event.clientX;
    rotationStart.value = rotation.value;
    window.addEventListener("pointermove", moveRotation);
    window.addEventListener("pointerup", endRotation, { once: true });
}

function moveRotation(event: PointerEvent) {
    if (!rotating.value) return;
    rotation.value = rotationStart.value + (event.clientX - rotationStart.x);
}

function endRotation() {
    rotating.value = false;
    window.removeEventListener("pointermove", moveRotation);
}

function startCrop(event: PointerEvent) {
    if (!draftUrl.value) return;
    event.preventDefault();
    panStart.x = pan.x;
    panStart.y = pan.y;
    panStart.pointerX = event.clientX;
    panStart.pointerY = event.clientY;
    cropping.value = true;
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
}

function moveCrop(event: PointerEvent) {
    if (!cropping.value) return;
    const limit = (CROP_WINDOW_SIZE / 2) * zoom.value;
    pan.x = Math.max(-limit, Math.min(limit, panStart.x + event.clientX - panStart.pointerX));
    pan.y = Math.max(-limit, Math.min(limit, panStart.y + event.clientY - panStart.pointerY));
}

function endCrop() {
    if (!cropping.value) return;
    cropping.value = false;
}

function handleFileSelect(event: Event) {
    const input = event.target as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;

    // Validate file type
    if (!file.type.startsWith('image/')) {
        alert('请上传图片文件');
        return;
    }

    // Validate file size (max 30MB)
    const maxSize = 30 * 1024 * 1024;
    if (file.size > maxSize) {
        alert('图片大小不能超过 30MB');
        return;
    }

    if (draftUrl.value.startsWith("blob:")) URL.revokeObjectURL(draftUrl.value);
    draftFile.value = file;
    draftUrl.value = URL.createObjectURL(file);
    zoom.value = 1;
    Object.assign(pan, { x: 0, y: 0 });

    // Reset input
    input.value = '';
}

async function confirmCrop() {
    if (!draftUrl.value) {
        emit("update:avatarUrl", "");
        closeEditor();
        return;
    }
    if (!draftFile.value) {
        closeEditor();
        return;
    }
    const image = new Image();
    image.src = draftUrl.value;
    await new Promise<void>((resolve, reject) => {
        image.onload = () => resolve();
        image.onerror = () => reject(new Error("无法读取图片"));
    });
    const rotated = document.createElement("canvas");
    const angle = (rotation.value * Math.PI) / 180;
    const cos = Math.abs(Math.cos(angle));
    const sin = Math.abs(Math.sin(angle));
    rotated.width = Math.ceil(image.naturalWidth * cos + image.naturalHeight * sin);
    rotated.height = Math.ceil(image.naturalWidth * sin + image.naturalHeight * cos);
    const rotatedContext = rotated.getContext("2d");
    if (!rotatedContext) return;
    rotatedContext.translate(rotated.width / 2, rotated.height / 2);
    rotatedContext.rotate(angle);
    rotatedContext.drawImage(image, -image.naturalWidth / 2, -image.naturalHeight / 2);
    const cropWidth = Math.min(rotated.width, rotated.height);
    const cropHeight = cropWidth;
    const cropX = Math.max(0, Math.min(rotated.width - cropWidth, Math.round((rotated.width - cropWidth) / 2 - (pan.x / CROP_WINDOW_SIZE) * rotated.width / zoom.value)));
    const cropY = Math.max(0, Math.min(rotated.height - cropHeight, Math.round((rotated.height - cropHeight) / 2 - (pan.y / CROP_WINDOW_SIZE) * rotated.height / zoom.value)));
    const canvas = document.createElement("canvas");
    canvas.width = 512;
    canvas.height = 512;
    const context = canvas.getContext("2d");
    if (!context) return;
    context.drawImage(rotated, cropX, cropY, cropWidth, cropHeight, 0, 0, 512, 512);
    const croppedFile = await new Promise<File>((resolve, reject) => {
        canvas.toBlob((blob) => blob ? resolve(new File([blob], "avatar.png", { type: "image/png" })) : reject(new Error("裁剪失败")), "image/png");
    });
    await uploadFile(croppedFile);
    closeEditor();
}

async function uploadFile(file: File) {
    if (uploading.value) return;

    uploading.value = true;
    try {
        const formData = new FormData();
        formData.append('file', file);

        const response = await fetch('/api/system/services/avatar', {
            method: 'POST',
            body: formData,
        });

        if (!response.ok) {
            const error = await response.text();
            throw new Error(error || '上传失败');
        }

        const result = await response.json();
        if (result.avatar_id) {
            // Store avatar:// prefix to distinguish from external URLs
            emit("update:avatarUrl", `avatar://${result.avatar_id}`);
        }
    } catch (e) {
        alert(`头像上传失败: ${e}`);
    } finally {
        uploading.value = false;
    }
}

function clearDraft() {
    if (uploading.value) return;
    if (draftUrl.value.startsWith("blob:")) URL.revokeObjectURL(draftUrl.value);
    draftUrl.value = "";
    draftFile.value = null;
    zoom.value = 1;
    rotation.value = 0;
    Object.assign(pan, { x: 0, y: 0 });
}
</script>
<style scoped lang="scss" src="./avatar-editor.scss"></style>
