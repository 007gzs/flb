<script setup lang="ts">
import type { StreamConfig } from '~/api'
import { ElMessage, ElMessageBox } from 'element-plus'
import { onMounted, reactive, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { api } from '~/api'

const { t } = useI18n()
const list = ref<StreamConfig[]>([])
const loading = ref(false)
const visible = ref(false)
const editing = ref<string | null>(null)
const form = reactive({
  name: '',
  listenPort: 9001,
  protocol: 'tcp' as StreamConfig['protocol'],
  targetIp: '',
  targetPort: 80,
})

async function load() {
  loading.value = true
  try {
    list.value = await api.streams.list()
  }
  finally {
    loading.value = false
  }
}

function openCreate() {
  editing.value = null
  form.name = ''
  form.listenPort = 9001
  form.protocol = 'tcp'
  form.targetIp = ''
  form.targetPort = 80
  visible.value = true
}

function openEdit(row: StreamConfig) {
  editing.value = row.id
  form.name = row.name
  form.listenPort = row.listenPort
  form.protocol = row.protocol
  form.targetIp = row.targetIp
  form.targetPort = row.targetPort
  visible.value = true
}

async function save() {
  try {
    const body = { ...form }
    if (editing.value)
      await api.streams.update(editing.value, body)
    else
      await api.streams.create(body)
    ElMessage.success(t('common.saved'))
    visible.value = false
    await load()
  }
  catch (e) {
    ElMessage.error((e as Error).message)
  }
}

async function remove(row: StreamConfig) {
  await ElMessageBox.confirm(t('streams.deleteConfirm', { name: row.name }), t('common.confirm'), { type: 'warning' })
  try {
    await api.streams.remove(row.id)
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
      <span>{{ t('streams.title') }}</span>
      <el-button type="primary" @click="openCreate">
        {{ t('streams.add') }}
      </el-button>
    </div>
    <el-table v-loading="loading" :data="list" stripe>
      <el-table-column prop="name" :label="t('common.name')" />
      <el-table-column prop="protocol" :label="t('streams.protocol')" width="90" />
      <el-table-column prop="listenPort" :label="t('streams.listenPort')" width="110" />
      <el-table-column prop="targetIp" :label="t('streams.targetIp')" />
      <el-table-column prop="targetPort" :label="t('streams.targetPort')" width="110" />
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
    <el-dialog v-model="visible" :title="editing ? t('streams.edit') : t('streams.add')" width="520px">
      <el-form label-width="110px">
        <el-form-item :label="t('common.name')">
          <el-input v-model="form.name" />
        </el-form-item>
        <el-form-item :label="t('streams.protocol')">
          <el-radio-group v-model="form.protocol">
            <el-radio value="tcp">
              TCP
            </el-radio>
            <el-radio value="udp">
              UDP
            </el-radio>
          </el-radio-group>
        </el-form-item>
        <el-form-item :label="t('streams.listenPort')">
          <el-input-number v-model="form.listenPort" :min="1" :max="65535" />
        </el-form-item>
        <el-form-item :label="t('streams.targetIp')">
          <el-input v-model="form.targetIp" />
        </el-form-item>
        <el-form-item :label="t('streams.targetPort')">
          <el-input-number v-model="form.targetPort" :min="1" :max="65535" />
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
