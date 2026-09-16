<script setup lang="ts">
import type { AppLocale } from '~/i18n'
import {
  ArrowDown,
  Connection,
  FolderOpened,
  House,
  Key,
  Monitor,
  Promotion,
} from '@element-plus/icons-vue'
import en from 'element-plus/es/locale/lang/en'
import zhCn from 'element-plus/es/locale/lang/zh-cn'
import { computed, onMounted, provide, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'
import { api } from '~/api'
import { setLocale } from '~/i18n'

const { t, locale } = useI18n()
const route = useRoute()
const router = useRouter()
const active = computed(() => (route.path === '/domains' ? '/certs' : route.path))
const epLocale = computed(() => (locale.value === 'zh-CN' ? zhCn : en))
const isLogin = computed(() => route.path === '/login')
const authed = ref(false)

const menus = computed(() => [
  { path: '/', label: t('nav.overview'), icon: House },
  { path: '/certs', label: t('nav.certs'), icon: Key },
  { path: '/dns', label: t('nav.dns'), icon: Promotion },
  { path: '/upstreams', label: t('nav.upstreams'), icon: Connection },
  { path: '/hosts', label: t('nav.hosts'), icon: Monitor },
  { path: '/streams', label: t('nav.streams'), icon: FolderOpened },
])

function onLocale(command: string) {
  setLocale(command as AppLocale)
}

function markAuthed() {
  authed.value = true
}

provide('markAuthed', markAuthed)

async function checkSession() {
  try {
    await api.me()
    authed.value = true
    if (isLogin.value)
      await router.replace('/')
  }
  catch {
    authed.value = false
    if (!isLogin.value)
      await router.replace('/login')
  }
}

async function logout() {
  try {
    await api.logout()
  }
  catch {
    // ignore
  }
  authed.value = false
  await router.replace('/login')
}

onMounted(checkSession)
</script>

<template>
  <el-config-provider :locale="epLocale" class="h-full">
    <router-view v-if="isLogin" />
    <el-container v-else-if="authed" class="h-full">
      <el-aside width="220px" class="aside">
        <div class="brand">
          <div class="brand-title">
            {{ t('app.brand') }}
          </div>
          <div class="brand-sub">
            {{ t('app.brandSub') }}
          </div>
        </div>
        <el-menu :default-active="active" router background-color="#0f172a" text-color="#cbd5e1" active-text-color="#38bdf8">
          <el-menu-item v-for="item in menus" :key="item.path" :index="item.path" @click="router.push(item.path)">
            <el-icon><component :is="item.icon" /></el-icon>
            <span>{{ item.label }}</span>
          </el-menu-item>
        </el-menu>
      </el-aside>
      <el-container>
        <el-header class="header">
          <span>{{ menus.find(m => m.path === active)?.label || t('app.title') }}</span>
          <div class="header-right">
            <el-dropdown trigger="click" @command="onLocale">
              <span class="lang-switch">
                {{ locale === 'zh-CN' ? t('lang.zh') : t('lang.en') }}
                <el-icon class="ml-4px"><ArrowDown /></el-icon>
              </span>
              <template #dropdown>
                <el-dropdown-menu>
                  <el-dropdown-item command="zh-CN" :disabled="locale === 'zh-CN'">
                    {{ t('lang.zh') }}
                  </el-dropdown-item>
                  <el-dropdown-item command="en-US" :disabled="locale === 'en-US'">
                    {{ t('lang.en') }}
                  </el-dropdown-item>
                </el-dropdown-menu>
              </template>
            </el-dropdown>
            <el-button link type="primary" @click="logout">
              {{ t('login.logout') }}
            </el-button>
          </div>
        </el-header>
        <el-main class="main">
          <router-view />
        </el-main>
      </el-container>
    </el-container>
  </el-config-provider>
</template>

<style scoped>
.aside {
  background: #0f172a;
  color: #fff;
}
.brand {
  padding: 20px 16px 12px;
}
.brand-title {
  font-size: 22px;
  font-weight: 700;
  letter-spacing: 1px;
}
.brand-sub {
  font-size: 12px;
  color: #94a3b8;
  margin-top: 4px;
}
.header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  background: #fff;
  border-bottom: 1px solid #ebeef5;
  font-size: 18px;
  font-weight: 600;
}
.header-right {
  display: flex;
  align-items: center;
  gap: 16px;
}
.lang-switch {
  display: inline-flex;
  align-items: center;
  font-size: 14px;
  font-weight: 500;
  color: #606266;
  cursor: pointer;
}
.main {
  padding: 20px;
}
</style>
