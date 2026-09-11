<script setup lang="ts">
import { onMounted, reactive, ref } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import { api, type DnsProvider } from '~/api'

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

const kinds = [
  { value: 'aliyun', label: '阿里云' },
  { value: 'wanwang', label: '万网' },
  { value: 'godaddy', label: 'GoDaddy' },
]

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
    ElMessage.success('已保存')
    visible.value = false
    await load()
  }
  catch (e) {
    ElMessage.error((e as Error).message)
  }
}

async function remove(row: DnsProvider) {
  await ElMessageBox.confirm(`删除 ${row.name}？`, '确认', { type: 'warning' })
  try {
    await api.dns.remove(row.id)
    ElMessage.success('已删除')
    await load()
  }
  catch (e) {
    ElMessage.error((e as Error).message)
  }
}

function kindLabel(kind: string) {
  return kinds.find(k => k.value === kind)?.label || kind
}

onMounted(load)
</script>

<template>
  <div class="page-card">
    <div class="page-header">
      <span>DNS 提供商</span>
      <el-button type="primary" @click="openCreate">添加提供商</el-button>
    </div>
    <el-table v-loading="loading" :data="list" stripe>
      <el-table-column prop="name" label="名称" />
      <el-table-column label="类型" width="140">
        <template #default="{ row }">{{ kindLabel(row.kind) }}</template>
      </el-table-column>
      <el-table-column prop="accessKey" label="Access Key" />
      <el-table-column label="操作" width="160">
        <template #default="{ row }">
          <el-button link type="primary" @click="openEdit(row)">编辑</el-button>
          <el-button link type="danger" @click="remove(row)">删除</el-button>
        </template>
      </el-table-column>
    </el-table>
    <el-dialog v-model="visible" :title="editing ? '编辑提供商' : '添加提供商'" width="520px">
      <el-form label-width="110px">
        <el-form-item label="名称">
          <el-input v-model="form.name" />
        </el-form-item>
        <el-form-item label="类型">
          <el-select v-model="form.kind" class="w-full">
            <el-option v-for="k in kinds" :key="k.value" :label="k.label" :value="k.value" />
          </el-select>
        </el-form-item>
        <el-form-item label="Access Key">
          <el-input v-model="form.accessKey" />
        </el-form-item>
        <el-form-item label="Access Secret">
          <el-input v-model="form.accessSecret" type="password" show-password />
        </el-form-item>
      </el-form>
      <template #footer>
        <el-button @click="visible = false">取消</el-button>
        <el-button type="primary" @click="save">保存</el-button>
      </template>
    </el-dialog>
  </div>
</template>
