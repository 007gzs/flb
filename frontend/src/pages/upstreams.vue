<script setup lang="ts">
import type { Upstream, UpstreamServer } from '~/api'
import { ElMessage, ElMessageBox } from 'element-plus'
import { onMounted, reactive, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { api } from '~/api'

const { t } = useI18n()

const list = ref<Upstream[]>([])
const loading = ref(false)
const visible = ref(false)
const editing = ref<string | null>(null)

type ServerDraft = Omit<UpstreamServer, 'port'> & { port?: number }

function emptyServer(): ServerDraft {
  return { address: '', weight: 1, protocol: 'http', verifyTls: false }
}

function portPlaceholder(protocol?: string) {
  return protocol === 'https' ? '443' : '80'
}

function parsePort(value: string) {
  const text = value.trim()
  if (!text)
    return undefined
  const port = Number(text)
  return Number.isInteger(port) ? port : undefined
}

function normalizeServer(s: UpstreamServer, fallback?: Upstream): ServerDraft {
  const protocol = s.protocol || fallback?.protocol || 'http'
  return {
    address: s.address,
    port: s.port || undefined,
    weight: s.weight || 1,
    protocol,
    verifyTls: s.verifyTls ?? (protocol === 'https' ? !!fallback?.verifyTls : false),
  }
}

const form = reactive({
  name: '',
  sni: '',
  servers: [emptyServer()] as ServerDraft[],
})

async function load() {
  loading.value = true
  try {
    list.value = await api.upstreams.list()
  }
  finally {
    loading.value = false
  }
}

function openCreate() {
  editing.value = null
  form.name = ''
  form.sni = ''
  form.servers = [emptyServer()]
  visible.value = true
}

function openEdit(row: Upstream) {
  editing.value = row.id
  form.name = row.name
  form.sni = row.sni || ''
  form.servers = row.servers.length
    ? row.servers.map(s => normalizeServer(s, row))
    : [emptyServer()]
  visible.value = true
}

function addServer() {
  form.servers.push(emptyServer())
}

function removeServer(idx: number) {
  form.servers.splice(idx, 1)
}

function serverLabel(s: UpstreamServer, fallback?: Upstream) {
  const protocol = s.protocol || fallback?.protocol || 'http'
  const port = s.port || portPlaceholder(protocol)
  return `${protocol}://${s.address}:${port}×${s.weight}`
}

async function save() {
  const servers: UpstreamServer[] = []
  for (const s of form.servers) {
    if (s.port != null && (s.port < 1 || s.port > 65535)) {
      ElMessage.error(t('upstreams.invalidPort'))
      return
    }
    servers.push({
      address: s.address,
      weight: s.weight,
      protocol: s.protocol,
      verifyTls: s.verifyTls,
      ...(s.port ? { port: s.port } : {}),
    })
  }
  try {
    const body = {
      name: form.name,
      sni: form.sni,
      servers,
    }
    if (editing.value)
      await api.upstreams.update(editing.value, body)
    else
      await api.upstreams.create(body)
    ElMessage.success(t('common.saved'))
    visible.value = false
    await load()
  }
  catch (e) {
    ElMessage.error((e as Error).message)
  }
}

async function remove(row: Upstream) {
  await ElMessageBox.confirm(t('upstreams.deleteConfirm', { name: row.name }), t('common.confirm'), { type: 'warning' })
  try {
    await api.upstreams.remove(row.id)
    ElMessage.success(t('common.deleted'))
    await load()
  }
  catch (e) {
    ElMessage.error((e as Error).message)
  }
}

onMounted(load)
</script>

<template>
  <div class="page-card">
    <div class="page-header">
      <span>{{ t('upstreams.title') }}</span>
      <el-button type="primary" @click="openCreate">
        {{ t('upstreams.add') }}
      </el-button>
    </div>
    <el-table v-loading="loading" :data="list" stripe>
      <el-table-column prop="name" :label="t('common.name')" />
      <el-table-column prop="sni" label="SNI" />
      <el-table-column :label="t('upstreams.backends')" min-width="320">
        <template #default="{ row }">
          {{ row.servers.map((s: UpstreamServer) => serverLabel(s, row)).join('，') }}
        </template>
      </el-table-column>
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
    <el-dialog v-model="visible" :title="editing ? t('upstreams.edit') : t('upstreams.add')" width="920px">
      <el-form label-width="90px">
        <el-form-item :label="t('common.name')">
          <el-input v-model="form.name" />
        </el-form-item>
        <el-form-item label="SNI">
          <el-input v-model="form.sni" :placeholder="t('upstreams.sniPlaceholder')" />
        </el-form-item>
        <el-form-item :label="t('upstreams.backends')">
          <div class="server-head">
            <span class="col-proto">{{ t('upstreams.protocol') }}</span>
            <span class="col-addr">{{ t('upstreams.address') }}</span>
            <span class="col-num">{{ t('upstreams.port') }}</span>
            <span class="col-num">{{ t('upstreams.weight') }}</span>
            <span class="col-verify">{{ t('upstreams.verifyTls') }}</span>
            <span class="col-act" />
          </div>
          <div v-for="(s, idx) in form.servers" :key="idx" class="server-row">
            <el-select v-model="s.protocol" class="col-proto">
              <el-option label="HTTP" value="http" />
              <el-option label="HTTPS" value="https" />
            </el-select>
            <el-input v-model="s.address" class="col-addr" :placeholder="t('upstreams.addressPlaceholder')" />
            <el-input
              class="col-num"
              :model-value="s.port == null ? '' : String(s.port)"
              :placeholder="portPlaceholder(s.protocol)"
              @update:model-value="(v: string) => (s.port = parsePort(v))"
            />
            <el-input-number v-model="s.weight" class="col-num" :min="1" :max="1000" controls-position="right" />
            <div class="col-verify">
              <el-switch v-if="s.protocol === 'https'" v-model="s.verifyTls" />
              <span v-else class="text-gray-400">-</span>
            </div>
            <el-button class="col-act" :disabled="form.servers.length === 1" @click="removeServer(idx)">
              {{ t('common.delete') }}
            </el-button>
          </div>
          <el-button @click="addServer">
            {{ t('upstreams.addServer') }}
          </el-button>
        </el-form-item>
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
.server-head,
.server-row {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 8px;
  width: 100%;
}
.server-head {
  color: #909399;
  font-size: 12px;
}
.col-proto {
  width: 110px;
  flex: none;
}
.col-addr {
  flex: 1;
  min-width: 0;
}
.col-num {
  width: 120px;
  flex: none;
}
.col-verify {
  width: 72px;
  flex: none;
  display: flex;
  justify-content: center;
}
.col-act {
  width: 64px;
  flex: none;
}
</style>
