<script setup lang="ts">
import type { Certificate, Domain, HeaderRewrite, Host, RouteRule, Upstream } from '~/api'
import { ElMessage, ElMessageBox } from 'element-plus'
import { computed, onMounted, reactive, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { api } from '~/api'

const { t } = useI18n()

const methods = ['GET', 'POST', 'PUT', 'DELETE', 'PATCH', 'HEAD', 'OPTIONS']
const list = ref<Host[]>([])
const certs = ref<Certificate[]>([])
const domains = ref<Domain[]>([])
const upstreams = ref<Upstream[]>([])
const loading = ref(false)
const visible = ref(false)
const editing = ref<string | null>(null)

function emptyRoute(): RouteRule {
  return {
    id: '',
    pattern: '/',
    matchType: 'prefix',
    rewriteUri: '',
    methods: [],
    requestHeaders: [],
    responseHeaders: [],
    upstreamId: '',
  }
}

const form = reactive({
  hostname: '',
  protocols: ['http'] as Array<'http' | 'https'>,
  certId: '',
  forceHttps: false,
  defaultUpstreamId: '',
  routes: [] as RouteRule[],
})

function protocolsFromHost(row: Host): Array<'http' | 'https'> {
  const raw = (row.protocol || '').toLowerCase().split(/[,+/| ]+/).filter(Boolean)
  const out: Array<'http' | 'https'> = []
  if (raw.includes('http'))
    out.push('http')
  if (row.httpsEnabled || raw.includes('https'))
    out.push('https')
  if (!out.length)
    out.push('http')
  return out
}

function hostnameLabel(row: Host) {
  return row.hostname?.trim() ? row.hostname : t('hosts.defaultCatchAll')
}

function protocolLabel(row: Host) {
  return protocolsFromHost(row).map(p => p.toUpperCase()).join(' / ')
}

function hasProtocol(name: 'http' | 'https') {
  return form.protocols.includes(name)
}

function onProtocolsChange() {
  if (!hasProtocol('http') || !hasProtocol('https'))
    form.forceHttps = false
}

const certOptions = computed(() => {
  const seen = new Set<string>()
  const options: { id: string, label: string }[] = []
  for (const domain of domains.value) {
    if (!domain.certId || seen.has(domain.certId))
      continue
    seen.add(domain.certId)
    const source = domain.mode === 'acme' ? t('certs.acme') : t('certs.manual')
    options.push({ id: domain.certId, label: `${domain.name} (${source})` })
  }
  for (const cert of certs.value) {
    if (seen.has(cert.id))
      continue
    options.push({ id: cert.id, label: cert.name })
  }
  return options
})

async function load() {
  loading.value = true
  try {
    ;[list.value, certs.value, domains.value, upstreams.value] = await Promise.all([
      api.hosts.list(),
      api.certs.list(),
      api.domains.list(),
      api.upstreams.list(),
    ])
  }
  finally {
    loading.value = false
  }
}

function openCreate() {
  editing.value = null
  form.hostname = ''
  form.protocols = ['http']
  form.certId = ''
  form.forceHttps = false
  form.defaultUpstreamId = ''
  form.routes = []
  visible.value = true
}

function openEdit(row: Host) {
  editing.value = row.id
  form.hostname = row.hostname
  form.protocols = protocolsFromHost(row)
  form.certId = row.certId || ''
  form.forceHttps = row.forceHttps
  form.defaultUpstreamId = row.defaultUpstreamId
  form.routes = row.routes.map(r => ({
    ...r,
    rewriteUri: r.rewriteUri || '',
    methods: [...r.methods],
    requestHeaders: r.requestHeaders.map(h => ({ ...h })),
    responseHeaders: r.responseHeaders.map(h => ({ ...h })),
  }))
  visible.value = true
}

function addRoute() {
  form.routes.push(emptyRoute())
}

function addHeader(listRef: HeaderRewrite[]) {
  listRef.push({ name: '', value: '' })
}

async function save() {
  const httpOn = hasProtocol('http')
  const httpsOn = hasProtocol('https')
  if (!httpOn && !httpsOn) {
    ElMessage.error(t('hosts.needProtocol'))
    return
  }
  if (httpsOn && !form.certId) {
    ElMessage.error(t('hosts.needCert'))
    return
  }
  if (form.forceHttps && !httpsOn) {
    ElMessage.error(t('hosts.needHttpsForRedirect'))
    return
  }
  try {
    const body = {
      hostname: form.hostname,
      protocol: httpOn && httpsOn ? 'http,https' : httpsOn ? 'https' : 'http',
      httpsEnabled: httpsOn,
      certId: httpsOn ? form.certId : undefined,
      forceHttps: httpOn && httpsOn && form.forceHttps,
      defaultUpstreamId: form.defaultUpstreamId,
      routes: form.routes.map(r => ({
        ...r,
        id: r.id || undefined,
        rewriteUri: r.rewriteUri || undefined,
      })),
    }
    if (editing.value)
      await api.hosts.update(editing.value, body)
    else
      await api.hosts.create(body)
    ElMessage.success(t('common.saved'))
    visible.value = false
    await load()
  }
  catch (e) {
    ElMessage.error((e as Error).message)
  }
}

async function remove(row: Host) {
  await ElMessageBox.confirm(t('hosts.deleteConfirm', { name: hostnameLabel(row) }), t('common.confirm'), { type: 'warning' })
  try {
    await api.hosts.remove(row.id)
    ElMessage.success(t('common.deleted'))
    await load()
  }
  catch (e) {
    ElMessage.error((e as Error).message)
  }
}

function upstreamName(id: string) {
  return upstreams.value.find(u => u.id === id)?.name || id
}

onMounted(load)
</script>

<template>
  <div class="page-card">
    <div class="page-header">
      <span>{{ t('hosts.title') }}</span>
      <el-button type="primary" @click="openCreate">
        {{ t('hosts.add') }}
      </el-button>
    </div>
    <el-table v-loading="loading" :data="list" stripe>
      <el-table-column :label="t('hosts.hostname')" min-width="220" show-overflow-tooltip>
        <template #default="{ row }">
          {{ hostnameLabel(row) }}
        </template>
      </el-table-column>
      <el-table-column :label="t('hosts.protocol')" width="140">
        <template #default="{ row }">
          {{ protocolLabel(row) }}
        </template>
      </el-table-column>
      <el-table-column :label="t('hosts.forceHttps')" width="130">
        <template #default="{ row }">
          {{ row.forceHttps ? t('common.yes') : t('common.no') }}
        </template>
      </el-table-column>
      <el-table-column :label="t('hosts.defaultUpstream')" min-width="140">
        <template #default="{ row }">
          {{ upstreamName(row.defaultUpstreamId) }}
        </template>
      </el-table-column>
      <el-table-column :label="t('hosts.routeCount')" width="90">
        <template #default="{ row }">
          {{ row.routes.length }}
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
    <el-dialog v-model="visible" :title="editing ? t('hosts.edit') : t('hosts.add')" width="920px" top="5vh">
      <el-form label-width="140px">
        <el-form-item :label="t('hosts.hostname')">
          <el-input v-model="form.hostname" type="textarea" :rows="3" :placeholder="t('hosts.hostnamePlaceholder')" />
        </el-form-item>
        <el-form-item :label="t('hosts.protocolType')">
          <el-checkbox-group v-model="form.protocols" @change="onProtocolsChange">
            <el-checkbox value="http">
              HTTP
            </el-checkbox>
            <el-checkbox value="https">
              HTTPS
            </el-checkbox>
          </el-checkbox-group>
        </el-form-item>
        <el-form-item v-if="hasProtocol('https')" :label="t('hosts.cert')">
          <el-select v-model="form.certId" class="w-full" filterable :placeholder="t('hosts.selectCert')">
            <el-option v-for="c in certOptions" :key="c.id" :label="c.label" :value="c.id" />
          </el-select>
        </el-form-item>
        <el-form-item v-if="hasProtocol('http') && hasProtocol('https')" :label="t('hosts.forceHttps')">
          <el-switch v-model="form.forceHttps" />
        </el-form-item>
        <el-form-item :label="t('hosts.defaultUpstream')">
          <el-select v-model="form.defaultUpstreamId" class="w-full">
            <el-option v-for="u in upstreams" :key="u.id" :label="u.name" :value="u.id" />
          </el-select>
        </el-form-item>
        <el-divider>{{ t('hosts.routes') }}</el-divider>
        <div v-for="(route, idx) in form.routes" :key="idx" class="route-box">
          <div class="mb-8px flex justify-between">
            <b>{{ t('hosts.routeN', { n: idx + 1 }) }}</b>
            <el-button link type="danger" @click="form.routes.splice(idx, 1)">
              {{ t('hosts.deleteRoute') }}
            </el-button>
          </div>
          <el-form-item :label="t('hosts.pattern')">
            <el-input v-model="route.pattern" :placeholder="t('hosts.patternPlaceholder')" />
          </el-form-item>
          <el-form-item :label="t('hosts.matchType')">
            <el-radio-group v-model="route.matchType">
              <el-radio value="prefix">
                {{ t('hosts.prefix') }}
              </el-radio>
              <el-radio value="regex">
                {{ t('hosts.regex') }}
              </el-radio>
            </el-radio-group>
          </el-form-item>
          <el-form-item :label="t('hosts.rewriteUri')">
            <el-input v-model="route.rewriteUri" :placeholder="t('hosts.rewritePlaceholder')" />
          </el-form-item>
          <el-form-item :label="t('hosts.methods')">
            <el-select v-model="route.methods" multiple class="w-full" :placeholder="t('hosts.methodsPlaceholder')">
              <el-option v-for="m in methods" :key="m" :label="m" :value="m" />
            </el-select>
          </el-form-item>
          <el-form-item :label="t('hosts.targetUpstream')">
            <el-select v-model="route.upstreamId" class="w-full">
              <el-option v-for="u in upstreams" :key="u.id" :label="u.name" :value="u.id" />
            </el-select>
          </el-form-item>
          <el-form-item :label="t('hosts.requestHeaders')">
            <div v-for="(h, hidx) in route.requestHeaders" :key="hidx" class="mb-8px w-full flex gap-8px">
              <el-input v-model="h.name" :placeholder="t('hosts.headerName')" />
              <el-input v-model="h.value" :placeholder="t('hosts.requestHeaderValue')" />
              <el-button @click="route.requestHeaders.splice(hidx, 1)">
                {{ t('hosts.removeHeader') }}
              </el-button>
            </div>
            <el-button @click="addHeader(route.requestHeaders)">
              {{ t('hosts.addRequestHeader') }}
            </el-button>
          </el-form-item>
          <el-form-item :label="t('hosts.responseHeaders')">
            <div v-for="(h, hidx) in route.responseHeaders" :key="hidx" class="mb-8px w-full flex gap-8px">
              <el-input v-model="h.name" :placeholder="t('hosts.headerName')" />
              <el-input v-model="h.value" :placeholder="t('hosts.responseHeaderValue')" />
              <el-button @click="route.responseHeaders.splice(hidx, 1)">
                {{ t('hosts.removeHeader') }}
              </el-button>
            </div>
            <el-button @click="addHeader(route.responseHeaders)">
              {{ t('hosts.addResponseHeader') }}
            </el-button>
          </el-form-item>
        </div>
        <el-button @click="addRoute">
          {{ t('hosts.addRoute') }}
        </el-button>
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
.route-box {
  border: 1px solid #ebeef5;
  border-radius: 8px;
  padding: 12px;
  margin-bottom: 12px;
}
</style>
