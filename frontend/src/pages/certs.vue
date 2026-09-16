<script setup lang="ts">
import type { Certificate, DnsProvider, Domain } from '~/api'
import { ElMessage, ElMessageBox } from 'element-plus'
import { computed, onMounted, reactive, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { api } from '~/api'

const { t } = useI18n()
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
  certPem: '',
  keyPem: '',
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

function linkedCert(row?: Domain | null) {
  const id = row?.certId
  return id ? certs.value.find(c => c.id === id) : undefined
}

function openCreate() {
  editing.value = null
  form.name = ''
  form.mode = 'acme'
  form.certPem = ''
  form.keyPem = ''
  form.challenge = 'http01'
  form.dnsProviderId = ''
  visible.value = true
}

function openEdit(row: Domain) {
  editing.value = row.id
  const cert = linkedCert(row)
  form.name = row.name
  form.mode = row.mode
  form.certPem = cert?.certPem || ''
  form.keyPem = cert?.keyPem || ''
  form.challenge = row.challenge || 'http01'
  form.dnsProviderId = row.dnsProviderId || ''
  visible.value = true
}

async function save() {
  if (isWildcard.value && form.mode === 'acme' && form.challenge === 'http01') {
    ElMessage.error(t('certs.wildcardHttp'))
    return
  }
  if (form.mode === 'manual' && (!form.certPem.trim() || !form.keyPem.trim())) {
    ElMessage.error(t('certs.needPem'))
    return
  }
  const body: Partial<Domain> = {
    name: form.name,
    mode: form.mode,
    certPem: form.mode === 'manual' ? form.certPem : undefined,
    keyPem: form.mode === 'manual' ? form.keyPem : undefined,
    challenge: form.mode === 'acme' ? form.challenge : undefined,
    dnsProviderId: form.mode === 'acme' && form.challenge === 'dns01' ? form.dnsProviderId : undefined,
  }
  try {
    if (editing.value)
      await api.domains.update(editing.value, body)
    else
      await api.domains.create(body)
    ElMessage.success(form.mode === 'acme' && !editing.value ? t('certs.issuing') : t('common.saved'))
    visible.value = false
    await load()
  }
  catch (e) {
    ElMessage.error((e as Error).message)
  }
}

async function remove(row: Domain) {
  await ElMessageBox.confirm(t('certs.deleteConfirm', { name: row.name }), t('common.confirm'), { type: 'warning' })
  try {
    await api.domains.remove(row.id)
    ElMessage.success(t('common.deleted'))
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
    ElMessage.success(t('certs.renewed'))
    await load()
  }
  catch (e) {
    ElMessage.error((e as Error).message)
  }
  finally {
    renewing.value = null
  }
}

onMounted(load)
</script>

<template>
  <div class="page-card">
    <div class="page-header">
      <span>{{ t('certs.title') }}</span>
      <el-button type="primary" @click="openCreate">
        {{ t('certs.add') }}
      </el-button>
    </div>
    <el-table v-loading="loading" :data="list" stripe>
      <el-table-column prop="name" :label="t('certs.domain')" min-width="180" />
      <el-table-column :label="t('certs.mode')" width="140">
        <template #default="{ row }">
          {{ row.mode === 'acme' ? t('certs.acme') : t('certs.manual') }}
        </template>
      </el-table-column>
      <el-table-column :label="t('certs.challenge')" width="120">
        <template #default="{ row }">
          {{ row.challenge === 'dns01' ? 'DNS' : row.challenge === 'http01' ? 'HTTP' : '-' }}
        </template>
      </el-table-column>
      <el-table-column :label="t('certs.status')" width="110">
        <template #default="{ row }">
          <el-tag :type="row.status === 'issued' ? 'success' : row.status === 'failed' ? 'danger' : 'info'">
            {{ row.status || '-' }}
          </el-tag>
        </template>
      </el-table-column>
      <el-table-column prop="expiresAt" :label="t('certs.expiresAt')" min-width="180" />
      <el-table-column prop="lastError" :label="t('certs.error')" min-width="180" show-overflow-tooltip />
      <el-table-column :label="t('common.actions')" width="220" fixed="right">
        <template #default="{ row }">
          <el-button v-if="row.mode === 'acme'" link type="success" :loading="renewing === row.id" @click="renew(row)">
            {{ t('certs.renew') }}
          </el-button>
          <el-button link type="primary" @click="openEdit(row)">
            {{ t('common.edit') }}
          </el-button>
          <el-button link type="danger" @click="remove(row)">
            {{ t('common.delete') }}
          </el-button>
        </template>
      </el-table-column>
    </el-table>
    <el-dialog v-model="visible" :title="editing ? t('certs.edit') : t('certs.add')" width="640px">
      <el-form label-width="140px">
        <el-form-item :label="t('certs.domain')">
          <el-input v-model="form.name" :placeholder="t('certs.namePlaceholder')" />
        </el-form-item>
        <el-form-item :label="t('certs.mode')">
          <el-radio-group v-model="form.mode">
            <el-radio value="acme">
              {{ t('certs.acmeAuto') }}
            </el-radio>
            <el-radio value="manual">
              {{ t('certs.manualUpload') }}
            </el-radio>
          </el-radio-group>
        </el-form-item>
        <template v-if="form.mode === 'manual'">
          <el-form-item :label="t('certs.certPem')">
            <el-input v-model="form.certPem" type="textarea" :rows="8" placeholder="-----BEGIN CERTIFICATE-----" />
          </el-form-item>
          <el-form-item :label="t('certs.keyPem')">
            <el-input v-model="form.keyPem" type="textarea" :rows="8" placeholder="-----BEGIN PRIVATE KEY-----" />
          </el-form-item>
        </template>
        <template v-else>
          <el-form-item :label="t('certs.challenge')">
            <el-radio-group v-model="form.challenge">
              <el-radio value="http01" :disabled="isWildcard">
                {{ t('certs.challengeHttp') }}
              </el-radio>
              <el-radio value="dns01">
                {{ t('certs.challengeDns') }}
              </el-radio>
            </el-radio-group>
          </el-form-item>
          <el-form-item v-if="form.challenge === 'dns01'" :label="t('nav.dns')">
            <el-select v-model="form.dnsProviderId" class="w-full">
              <el-option v-for="p in providers" :key="p.id" :label="p.name" :value="p.id" />
            </el-select>
          </el-form-item>
        </template>
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
