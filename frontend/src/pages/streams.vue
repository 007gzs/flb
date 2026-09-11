<script setup lang="ts">
import { onMounted, reactive, ref } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import { api, type StreamConfig } from '~/api'

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
    ElMessage.success('已保存')
    visible.value = false
    await load()
  }
  catch (e) {
    ElMessage.error((e as Error).message)
  }
}

async function remove(row: StreamConfig) {
  await ElMessageBox.confirm(`删除数据流 ${row.name}？`, '确认', { type: 'warning' })
  try {
    await api.streams.remove(row.id)
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
      <span>TCP / UDP 数据流</span>
      <el-button type="primary" @click="openCreate">添加数据流</el-button>
    </div>
    <el-table v-loading="loading" :data="list" stripe>
      <el-table-column prop="name" label="名称" />
      <el-table-column prop="protocol" label="协议" width="90" />
      <el-table-column prop="listenPort" label="监听端口" width="110" />
      <el-table-column prop="targetIp" label="目标 IP" />
      <el-table-column prop="targetPort" label="目标端口" width="110" />
      <el-table-column label="操作" width="160">
        <template #default="{ row }">
          <el-button link type="primary" @click="openEdit(row)">编辑</el-button>
          <el-button link type="danger" @click="remove(row)">删除</el-button>
        </template>
      </el-table-column>
    </el-table>
    <el-dialog v-model="visible" :title="editing ? '编辑数据流' : '添加数据流'" width="520px">
      <el-form label-width="110px">
        <el-form-item label="名称">
          <el-input v-model="form.name" />
        </el-form-item>
        <el-form-item label="协议">
          <el-radio-group v-model="form.protocol">
            <el-radio value="tcp">TCP</el-radio>
            <el-radio value="udp">UDP</el-radio>
          </el-radio-group>
        </el-form-item>
        <el-form-item label="监听端口">
          <el-input-number v-model="form.listenPort" :min="1" :max="65535" />
        </el-form-item>
        <el-form-item label="目标 IP">
          <el-input v-model="form.targetIp" />
        </el-form-item>
        <el-form-item label="目标端口">
          <el-input-number v-model="form.targetPort" :min="1" :max="65535" />
        </el-form-item>
      </el-form>
      <template #footer>
        <el-button @click="visible = false">取消</el-button>
        <el-button type="primary" @click="save">保存</el-button>
      </template>
    </el-dialog>
  </div>
</template>
