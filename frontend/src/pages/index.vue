<script setup lang="ts">
import type { Stats } from '~/api'
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { api } from '~/api'

const { t } = useI18n()
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

const cards = computed(() => [
  { key: 'certs' as const, label: t('nav.certs'), path: '/certs' },
  { key: 'dnsProviders' as const, label: t('nav.dns'), path: '/dns' },
  { key: 'domains' as const, label: t('nav.domains'), path: '/domains' },
  { key: 'upstreams' as const, label: t('nav.upstreams'), path: '/upstreams' },
  { key: 'hosts' as const, label: t('overview.hosts'), path: '/hosts' },
  { key: 'streams' as const, label: t('nav.streams'), path: '/streams' },
])
</script>

<template>
  <div v-loading="loading" class="grid grid-cols-1 gap-16px md:grid-cols-3">
    <el-card v-for="card in cards" :key="card.key" shadow="hover" class="cursor-pointer" @click="$router.push(card.path)">
      <div class="text-13px text-gray-500">
        {{ card.label }}
      </div>
      <div class="mt-8px text-32px font-700">
        {{ stats ? stats[card.key] : '-' }}
      </div>
    </el-card>
  </div>
</template>
