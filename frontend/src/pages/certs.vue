<script setup lang="ts">
import type { Certificate } from '~/api'
import { ElMessage, ElMessageBox } from 'element-plus'
import { onMounted, reactive, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { api } from '~/api'

const { t } = useI18n()
const list = ref<Certificate[]>([])
const loading = ref(false)
const visible = ref(false)
const editing = ref<string | null>(null)
const form = reactive({
  name: '',
  certPem: '',
  keyPem: '',
})

async function load() {
  loading.value = true
  try {
    list.value = await api.certs.list()
  }
  finally {
    loading.value = false
  }
}

function openCreate() {
  editing.value = null
  form.name = ''
  form.certPem = ''
  form.keyPem = ''
  visible.value = true
}

function openEdit(row: Certificate) {
  editing.value = row.id
  form.name = row.name
  form.certPem = row.certPem
  form.keyPem = row.keyPem
  visible.value = true
}

async function save() {
  const body = { name: form.name, certPem: form.certPem, keyPem: form.keyPem }
  try {
    if (editing.value)
      await api.certs.update(editing.value, body)
    else
      await api.certs.create(body)
    ElMessage.success(t('common.saved'))
    visible.value = false
    await load()
  }
  catch (e) {
    ElMessage.error((e as Error).message)
  }
}

async function remove(row: Certificate) {
  await ElMessageBox.confirm(t('certs.deleteConfirm', { name: row.name }), t('common.confirm'), { type: 'warning' })
  try {
    await api.certs.remove(row.id)
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
      <span>{{ t('certs.title') }}</span>
      <el-button type="primary" @click="openCreate">
        {{ t('certs.add') }}
      </el-button>
    </div>
    <el-table v-loading="loading" :data="list" stripe>
      <el-table-column prop="name" :label="t('common.name')" min-width="160" />
      <el-table-column :label="t('certs.source')" width="120">
        <template #default="{ row }">
          {{ row.autoIssued ? 'Let\'s Encrypt' : t('certs.manual') }}
        </template>
      </el-table-column>
      <el-table-column prop="notAfter" :label="t('certs.notAfter')" min-width="180" />
      <el-table-column prop="createdAt" :label="t('certs.createdAt')" min-width="180" />
      <el-table-column :label="t('common.actions')" width="160" fixed="right">
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
    <el-dialog v-model="visible" :title="editing ? t('certs.edit') : t('certs.add')" width="640px">
      <el-form label-width="110px">
        <el-form-item :label="t('common.name')">
          <el-input v-model="form.name" />
        </el-form-item>
        <el-form-item :label="t('certs.certPem')">
          <el-input v-model="form.certPem" type="textarea" :rows="8" placeholder="-----BEGIN CERTIFICATE-----" />
        </el-form-item>
        <el-form-item :label="t('certs.keyPem')">
          <el-input v-model="form.keyPem" type="textarea" :rows="8" placeholder="-----BEGIN PRIVATE KEY-----" />
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
