<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { api, type Stats } from '~/api'

const stats = ref<Stats | null>(null)
const loading = ref(false)

async function load() {
  loading.value = true
  try {
    stats.value = await api.stats()
  }
  finally {
    loading.value = false
  }
}

onMounted(load)

const cards = [
  { key: 'certs', label: '证书', path: '/certs' },
  { key: 'dnsProviders', label: 'DNS 提供商', path: '/dns' },
  { key: 'domains', label: '域名证书', path: '/domains' },
  { key: 'upstreams', label: '后端服务组', path: '/upstreams' },
  { key: 'hosts', label: '主机', path: '/hosts' },
  { key: 'streams', label: '数据流', path: '/streams' },
] as const
</script>

<template>
  <div v-loading="loading" class="grid grid-cols-1 md:grid-cols-3 gap-16px">
    <el-card v-for="card in cards" :key="card.key" shadow="hover" class="cursor-pointer" @click="$router.push(card.path)">
      <div class="text-13px text-gray-500">{{ card.label }}</div>
      <div class="text-32px font-700 mt-8px">
        {{ stats ? stats[card.key] : '-' }}
      </div>
    </el-card>
  </div>
</template>
