<script setup lang="ts">
import { computed } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import {
  Connection,
  FolderOpened,
  House,
  Key,
  Link,
  Monitor,
  Promotion,
} from '@element-plus/icons-vue'

const route = useRoute()
const router = useRouter()
const active = computed(() => route.path)

const menus = [
  { path: '/', label: '概览', icon: House },
  { path: '/certs', label: '证书', icon: Key },
  { path: '/dns', label: 'DNS 提供商', icon: Promotion },
  { path: '/domains', label: '域名证书', icon: Link },
  { path: '/upstreams', label: '后端服务组', icon: Connection },
  { path: '/hosts', label: '主机配置', icon: Monitor },
  { path: '/streams', label: '数据流', icon: FolderOpened },
]
</script>

<template>
  <el-container class="h-full">
    <el-aside width="220px" class="aside">
      <div class="brand">
        <div class="brand-title">FLB</div>
        <div class="brand-sub">Fast Load Balancing</div>
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
        {{ menus.find(m => m.path === active)?.label || '配置管理' }}
      </el-header>
      <el-main class="main">
        <router-view />
      </el-main>
    </el-container>
  </el-container>
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
  background: #fff;
  border-bottom: 1px solid #ebeef5;
  font-size: 18px;
  font-weight: 600;
}
.main {
  padding: 20px;
}
</style>
