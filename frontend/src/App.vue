<script setup lang="ts">
import type { AppLocale } from '~/i18n'
import {
  ArrowDown,
  Connection,
  FolderOpened,
  House,
  Key,
  Link,
  Monitor,
  Promotion,
} from '@element-plus/icons-vue'
import en from 'element-plus/es/locale/lang/en'
import zhCn from 'element-plus/es/locale/lang/zh-cn'
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'
import { setLocale } from '~/i18n'

const { t, locale } = useI18n()
const route = useRoute()
const router = useRouter()
const active = computed(() => route.path)
const epLocale = computed(() => (locale.value === 'zh-CN' ? zhCn : en))

const menus = computed(() => [
  { path: '/', label: t('nav.overview'), icon: House },
  { path: '/certs', label: t('nav.certs'), icon: Key },
  { path: '/dns', label: t('nav.dns'), icon: Promotion },
  { path: '/domains', label: t('nav.domains'), icon: Link },
  { path: '/upstreams', label: t('nav.upstreams'), icon: Connection },
  { path: '/hosts', label: t('nav.hosts'), icon: Monitor },
  { path: '/streams', label: t('nav.streams'), icon: FolderOpened },
])

function onLocale(command: string) {
  setLocale(command as AppLocale)
}
</script>

<template>
  <el-config-provider :locale="epLocale" class="h-full">
    <el-container class="h-full">
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
