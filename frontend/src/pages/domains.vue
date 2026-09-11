<script setup lang="ts">
import { computed, onMounted, reactive, ref } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import { api, type Certificate, type DnsProvider, type Domain } from '~/api'

const list = ref<Domain[]>([])
const certs = ref<Certificate[]>([])
const providers = ref<DnsProvider[]>([])
const loading = ref(false)
const renewing = ref<string | null>(null)
const visible = ref(false)
const editing = ref<string | null>(null)
const form = reactive({
  name: '',
  mode: 'acme' as Domain['mode'],
  certId: '',
  challenge: 'http01' as 'http01' | 'dns01',
  dnsProviderId: '',
})

const isWildcard = computed(() => form.name.trim().startsWith('*.'))

async function load() {
  loading.value = true
  try {
    ;[list.value, certs.value, providers.value] = await Promise.all([
      api.domains.list(),
      api.certs.list(),
      api.dns.list(),
    ])
  }
  finally {
    loading.value = false
  }
}

function openCreate() {
  editing.value = null
  form.name = ''
  form.mode = 'acme'
  form.certId = ''
  form.challenge = 'http01'
  form.dnsProviderId = ''
  visible.value = true
}

function openEdit(row: Domain) {
  editing.value = row.id
  form.name = row.name
  form.mode = row.mode
  form.certId = row.certId || ''
  form.challenge = row.challenge || 'http01'
  form.dnsProviderId = row.dnsProviderId || ''
  visible.value = true
}

async function save() {
  if (isWildcard.value && form.mode === 'acme' && form.challenge === 'http01') {
    ElMessage.error('泛域名不支持 HTTP 验证')
    return
  }
  const body = {
    name: form.name,
    mode: form.mode,
    certId: form.mode === 'manual' ? form.certId : undefined,
    challenge: form.mode === 'acme' ? form.challenge : undefined,
    dnsProviderId: form.mode === 'acme' && form.challenge === 'dns01' ? form.dnsProviderId : undefined,
  }
  try {
    if (editing.value)
      await api.domains.update(editing.value, body)
    else
      await api.domains.create(body)
    ElMessage.success(form.mode === 'acme' && !editing.value ? '已提交，正在申请证书' : '已保存')
    visible.value = false
    await load()
  }
  catch (e) {
    ElMessage.error((e as Error).message)
  }
}

async function remove(row: Domain) {
  await ElMessageBox.confirm(`删除域名 ${row.name}？`, '确认', { type: 'warning' })
  try {
    await api.domains.remove(row.id)
    ElMessage.success('已删除')
    await load()
  }
  catch (e) {
    ElMessage.error((e as Error).message)
  }
}

async function renew(row: Domain) {
  renewing.value = row.id
  try {
    await api.domains.renew(row.id)
    ElMessage.success('续签完成')
    await load()
  }
  catch (e) {
    ElMessage.error((e as Error).message)
  }
  finally {
    renewing.value = null
  }
}

function certName(id?: string | null) {
  return certs.value.find(c => c.id === id)?.name || id || '-'
}

onMounted(load)
</script>

<template>
  <div class="page-card">
    <div class="page-header">
      <span>域名证书</span>
      <el-button type="primary" @click="openCreate">添加域名</el-button>
    </div>
    <el-table v-loading="loading" :data="list" stripe>
      <el-table-column prop="name" label="域名" min-width="180" />
      <el-table-column label="证书方式" width="130">
        <template #default="{ row }">{{ row.mode === 'acme' ? 'Let\'s Encrypt' : '手动' }}</template>
      </el-table-column>
      <el-table-column label="验证方式" width="120">
        <template #default="{ row }">
          {{ row.challenge === 'dns01' ? 'DNS' : row.challenge === 'http01' ? 'HTTP' : '-' }}
        </template>
      </el-table-column>
      <el-table-column label="状态" width="110">
        <template #default="{ row }">
          <el-tag :type="row.status === 'issued' ? 'success' : row.status === 'failed' ? 'danger' : 'info'">
            {{ row.status || '-' }}
          </el-tag>
        </template>
      </el-table-column>
      <el-table-column label="证书" min-width="140">
        <template #default="{ row }">{{ certName(row.certId) }}</template>
      </el-table-column>
      <el-table-column prop="expiresAt" label="过期时间" min-width="180" />
      <el-table-column prop="lastError" label="错误" min-width="180" show-overflow-tooltip />
      <el-table-column label="操作" width="220" fixed="right">
        <template #default="{ row }">
          <el-button v-if="row.mode === 'acme'" link type="success" :loading="renewing === row.id" @click="renew(row)">续签</el-button>
          <el-button link type="primary" @click="openEdit(row)">编辑</el-button>
          <el-button link type="danger" @click="remove(row)">删除</el-button>
        </template>
      </el-table-column>
    </el-table>
    <el-dialog v-model="visible" :title="editing ? '编辑域名' : '添加域名'" width="560px">
      <el-form label-width="120px">
        <el-form-item label="域名">
          <el-input v-model="form.name" placeholder="example.com 或 *.example.com" />
        </el-form-item>
        <el-form-item label="证书方式">
          <el-radio-group v-model="form.mode">
            <el-radio value="acme">自动 Let's Encrypt</el-radio>
            <el-radio value="manual">手动选择证书</el-radio>
          </el-radio-group>
        </el-form-item>
        <el-form-item v-if="form.mode === 'manual'" label="证书">
          <el-select v-model="form.certId" class="w-full" filterable>
            <el-option v-for="c in certs" :key="c.id" :label="c.name" :value="c.id" />
          </el-select>
        </el-form-item>
        <template v-if="form.mode === 'acme'">
          <el-form-item label="验证方式">
            <el-radio-group v-model="form.challenge">
              <el-radio value="http01" :disabled="isWildcard">HTTP 验证</el-radio>
              <el-radio value="dns01">DNS 提供商</el-radio>
            </el-radio-group>
          </el-form-item>
          <el-form-item v-if="form.challenge === 'dns01'" label="DNS 提供商">
            <el-select v-model="form.dnsProviderId" class="w-full">
              <el-option v-for="p in providers" :key="p.id" :label="p.name" :value="p.id" />
            </el-select>
          </el-form-item>
        </template>
      </el-form>
      <template #footer>
        <el-button @click="visible = false">取消</el-button>
        <el-button type="primary" @click="save">保存</el-button>
      </template>
    </el-dialog>
  </div>
</template>
