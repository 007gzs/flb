<script setup lang="ts">
import type { DnsProvider } from '~/api'
import { ElMessage, ElMessageBox } from 'element-plus'
import { computed, onMounted, reactive, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { api } from '~/api'

const { t } = useI18n()
const list = ref<DnsProvider[]>([])
const loading = ref(false)
const visible = ref(false)
const editing = ref<string | null>(null)
const form = reactive({
  name: '',
  kind: 'aliyun' as DnsProvider['kind'],
  accessKey: '',
  accessSecret: '',
})

const kinds = computed(() => [
  { value: 'aliyun' as const, label: t('dns.kind.aliyun') },
  { value: 'wanwang' as const, label: t('dns.kind.wanwang') },
  { value: 'tencent' as const, label: t('dns.kind.tencent') },
  { value: 'xinnet' as const, label: t('dns.kind.xinnet') },
  { value: 'cloudflare' as const, label: t('dns.kind.cloudflare') },
  { value: 'amazon' as const, label: t('dns.kind.amazon') },
  { value: 'godaddy' as const, label: t('dns.kind.godaddy') },
])

const kindHint = computed(() => {
  const hints: Partial<Record<DnsProvider['kind'], string>> = {
    tencent: t('dns.hint.tencent'),
    xinnet: t('dns.hint.xinnet'),
    cloudflare: t('dns.hint.cloudflare'),
    amazon: t('dns.hint.amazon'),
  }
  return hints[form.kind] || ''
})

async function load() {
  loading.value = true
  try {
    list.value = await api.dns.list()
  }
  finally {
    loading.value = false
  }
}

function openCreate() {
  editing.value = null
  form.name = ''
  form.kind = 'aliyun'
  form.accessKey = ''
  form.accessSecret = ''
  visible.value = true
}

function openEdit(row: DnsProvider) {
  editing.value = row.id
  form.name = row.name
  form.kind = row.kind
  form.accessKey = row.accessKey
  form.accessSecret = row.accessSecret
  visible.value = true
}

async function save() {
  try {
    const body = { ...form }
    if (editing.value)
      await api.dns.update(editing.value, body)
    else
      await api.dns.create(body)
    ElMessage.success(t('common.saved'))
    visible.value = false
    await load()
  }
  catch (e) {
    ElMessage.error((e as Error).message)
  }
}

async function remove(row: DnsProvider) {
  await ElMessageBox.confirm(t('dns.deleteConfirm', { name: row.name }), t('common.confirm'), { type: 'warning' })
  try {
    await api.dns.remove(row.id)
    ElMessage.success(t('common.deleted'))
    await load()
  }
  catch (e) {
    ElMessage.error((e as Error).message)
  }
}

function kindLabel(kind: string) {
  return kinds.value.find(k => k.value === kind)?.label || kind
}

onMounted(load)
</script>

<template>
  <div class="page-card">
    <div class="page-header">
      <span>{{ t('dns.title') }}</span>
      <el-button type="primary" @click="openCreate">
        {{ t('dns.add') }}
      </el-button>
    </div>
    <el-table v-loading="loading" :data="list" stripe>
      <el-table-column prop="name" :label="t('common.name')" />
      <el-table-column :label="t('common.type')" width="160">
        <template #default="{ row }">
          {{ kindLabel(row.kind) }}
        </template>
      </el-table-column>
      <el-table-column prop="accessKey" label="Access Key" />
      <el-table-column :label="t('common.actions')" width="160">
        <template #default="{ row }">
          <el-button link type="primary" @click="openEdit(row)">
            {{ t('common.edit') }}
          </el-button>
          <el-button link type="danger" @click="remove(row)">
            {{ t('common.delete') }}
          </el-button>
        </template>
      </el-table-column>
    </el-table>
    <el-dialog v-model="visible" :title="editing ? t('dns.edit') : t('dns.add')" width="520px">
      <el-form label-width="110px">
        <el-form-item :label="t('common.name')">
          <el-input v-model="form.name" />
        </el-form-item>
        <el-form-item :label="t('common.type')">
          <el-select v-model="form.kind" class="w-full">
            <el-option v-for="k in kinds" :key="k.value" :label="k.label" :value="k.value" />
          </el-select>
        </el-form-item>
        <el-form-item :label="t('dns.accessKey')">
          <el-input v-model="form.accessKey" />
        </el-form-item>
        <el-form-item :label="t('dns.accessSecret')">
          <el-input v-model="form.accessSecret" type="password" show-password />
        </el-form-item>
        <p v-if="kindHint" class="hint">
          {{ kindHint }}
        </p>
      </el-form>
      <template #footer>
        <el-button @click="visible = false">
          {{ t('common.cancel') }}
        </el-button>
        <el-button type="primary" @click="save">
          {{ t('common.save') }}
        </el-button>
      </template>
    </el-dialog>
  </div>
</template>

<style scoped>
.hint {
  margin: 0 0 0 110px;
  color: #909399;
  font-size: 12px;
  line-height: 1.5;
}
</style>
