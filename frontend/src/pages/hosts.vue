<script setup lang="ts">
import { onMounted, reactive, ref } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import { api, type Certificate, type HeaderRewrite, type Host, type RouteRule, type Upstream } from '~/api'

const methods = ['GET', 'POST', 'PUT', 'DELETE', 'PATCH', 'HEAD', 'OPTIONS']
const list = ref<Host[]>([])
const certs = ref<Certificate[]>([])
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

async function load() {
  loading.value = true
  try {
    ;[list.value, certs.value, upstreams.value] = await Promise.all([
      api.hosts.list(),
      api.certs.list(),
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
    ElMessage.error('请至少勾选一种协议')
    return
  }
  if (httpsOn && !form.certId) {
    ElMessage.error('勾选 HTTPS 时需要选择证书')
    return
  }
  if (form.forceHttps && !httpsOn) {
    ElMessage.error('HTTP 转 HTTPS 需要同时勾选 HTTPS')
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
    ElMessage.success('已保存')
    visible.value = false
    await load()
  }
  catch (e) {
    ElMessage.error((e as Error).message)
  }
}

async function remove(row: Host) {
  await ElMessageBox.confirm(`删除主机 ${row.hostname}？`, '确认', { type: 'warning' })
  try {
    await api.hosts.remove(row.id)
    ElMessage.success('已删除')
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
      <span>主机配置</span>
      <el-button type="primary" @click="openCreate">添加主机</el-button>
    </div>
    <el-table v-loading="loading" :data="list" stripe>
      <el-table-column prop="hostname" label="主机域名" min-width="180" />
      <el-table-column label="协议" width="140">
        <template #default="{ row }">{{ protocolLabel(row) }}</template>
      </el-table-column>
      <el-table-column label="HTTP 转 HTTPS" width="130">
        <template #default="{ row }">{{ row.forceHttps ? '是' : '否' }}</template>
      </el-table-column>
      <el-table-column label="默认服务组" min-width="140">
        <template #default="{ row }">{{ upstreamName(row.defaultUpstreamId) }}</template>
      </el-table-column>
      <el-table-column label="路由数" width="90">
        <template #default="{ row }">{{ row.routes.length }}</template>
      </el-table-column>
      <el-table-column label="操作" width="160">
        <template #default="{ row }">
          <el-button link type="primary" @click="openEdit(row)">编辑</el-button>
          <el-button link type="danger" @click="remove(row)">删除</el-button>
        </template>
      </el-table-column>
    </el-table>
    <el-dialog v-model="visible" :title="editing ? '编辑主机' : '添加主机'" width="920px" top="5vh">
      <el-form label-width="130px">
        <el-form-item label="主机域名">
          <el-input v-model="form.hostname" placeholder="app.example.com 或 *.example.com" />
        </el-form-item>
        <el-form-item label="协议类型">
          <el-checkbox-group v-model="form.protocols" @change="onProtocolsChange">
            <el-checkbox value="http">HTTP</el-checkbox>
            <el-checkbox value="https">HTTPS</el-checkbox>
          </el-checkbox-group>
        </el-form-item>
        <el-form-item v-if="hasProtocol('https')" label="证书">
          <el-select v-model="form.certId" class="w-full" filterable placeholder="选择证书">
            <el-option v-for="c in certs" :key="c.id" :label="c.name" :value="c.id" />
          </el-select>
        </el-form-item>
        <el-form-item v-if="hasProtocol('http') && hasProtocol('https')" label="HTTP 转 HTTPS">
          <el-switch v-model="form.forceHttps" />
        </el-form-item>
        <el-form-item label="默认服务组">
          <el-select v-model="form.defaultUpstreamId" class="w-full">
            <el-option v-for="u in upstreams" :key="u.id" :label="u.name" :value="u.id" />
          </el-select>
        </el-form-item>
        <el-divider>路由管理</el-divider>
        <div v-for="(route, idx) in form.routes" :key="idx" class="route-box">
          <div class="flex justify-between mb-8px">
            <b>路由 {{ idx + 1 }}</b>
            <el-button link type="danger" @click="form.routes.splice(idx, 1)">删除路由</el-button>
          </div>
          <el-form-item label="匹配内容">
            <el-input v-model="route.pattern" placeholder="前缀如 /api 或正则" />
          </el-form-item>
          <el-form-item label="匹配规则">
            <el-radio-group v-model="route.matchType">
              <el-radio value="prefix">前缀匹配</el-radio>
              <el-radio value="regex">正则匹配</el-radio>
            </el-radio-group>
          </el-form-item>
          <el-form-item label="跳转 URI">
            <el-input v-model="route.rewriteUri" placeholder="支持 $1 等正则变量，前缀可写 /new" />
          </el-form-item>
          <el-form-item label="支持方法">
            <el-select v-model="route.methods" multiple class="w-full" placeholder="空表示全部">
              <el-option v-for="m in methods" :key="m" :label="m" :value="m" />
            </el-select>
          </el-form-item>
          <el-form-item label="目标服务组">
            <el-select v-model="route.upstreamId" class="w-full">
              <el-option v-for="u in upstreams" :key="u.id" :label="u.name" :value="u.id" />
            </el-select>
          </el-form-item>
          <el-form-item label="请求头重写">
            <div v-for="(h, hidx) in route.requestHeaders" :key="hidx" class="flex gap-8px mb-8px w-full">
              <el-input v-model="h.name" placeholder="Header 名" />
              <el-input v-model="h.value" placeholder="值，可用 $http_host；空则删除" />
              <el-button @click="route.requestHeaders.splice(hidx, 1)">删</el-button>
            </div>
            <el-button @click="addHeader(route.requestHeaders)">添加请求头</el-button>
          </el-form-item>
          <el-form-item label="返回头重写">
            <div v-for="(h, hidx) in route.responseHeaders" :key="hidx" class="flex gap-8px mb-8px w-full">
              <el-input v-model="h.name" placeholder="Header 名" />
              <el-input v-model="h.value" placeholder="值，可用 $upstream_http_server" />
              <el-button @click="route.responseHeaders.splice(hidx, 1)">删</el-button>
            </div>
            <el-button @click="addHeader(route.responseHeaders)">添加返回头</el-button>
          </el-form-item>
        </div>
        <el-button @click="addRoute">添加路由</el-button>
      </el-form>
      <template #footer>
        <el-button @click="visible = false">取消</el-button>
        <el-button type="primary" @click="save">保存</el-button>
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
