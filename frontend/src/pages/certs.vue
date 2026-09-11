<script setup lang="ts">
import { onMounted, reactive, ref } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import { api, type Certificate } from '~/api'

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
    ElMessage.success('已保存')
    visible.value = false
    await load()
  }
  catch (e) {
    ElMessage.error((e as Error).message)
  }
}

async function remove(row: Certificate) {
  await ElMessageBox.confirm(`删除证书 ${row.name}？`, '确认', { type: 'warning' })
  try {
    await api.certs.remove(row.id)
    ElMessage.success('已删除')
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
      <span>证书管理</span>
      <el-button type="primary" @click="openCreate">添加证书</el-button>
    </div>
    <el-table v-loading="loading" :data="list" stripe>
      <el-table-column prop="name" label="名称" min-width="160" />
      <el-table-column label="来源" width="120">
        <template #default="{ row }">{{ row.autoIssued ? 'Let\'s Encrypt' : '手动' }}</template>
      </el-table-column>
      <el-table-column prop="notAfter" label="过期时间" min-width="180" />
      <el-table-column prop="createdAt" label="创建时间" min-width="180" />
      <el-table-column label="操作" width="160" fixed="right">
        <template #default="{ row }">
          <el-button link type="primary" @click="openEdit(row)">编辑</el-button>
          <el-button link type="danger" @click="remove(row)">删除</el-button>
        </template>
      </el-table-column>
    </el-table>
    <el-dialog v-model="visible" :title="editing ? '编辑证书' : '添加证书'" width="640px">
      <el-form label-width="90px">
        <el-form-item label="名称">
          <el-input v-model="form.name" />
        </el-form-item>
        <el-form-item label="公钥 PEM">
          <el-input v-model="form.certPem" type="textarea" :rows="8" placeholder="-----BEGIN CERTIFICATE-----" />
        </el-form-item>
        <el-form-item label="私钥 PEM">
          <el-input v-model="form.keyPem" type="textarea" :rows="8" placeholder="-----BEGIN PRIVATE KEY-----" />
        </el-form-item>
      </el-form>
      <template #footer>
        <el-button @click="visible = false">取消</el-button>
        <el-button type="primary" @click="save">保存</el-button>
      </template>
    </el-dialog>
  </div>
</template>
