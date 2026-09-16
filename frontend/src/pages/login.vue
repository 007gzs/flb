<script setup lang="ts">
import type { AppLocale } from '~/i18n'
import { ArrowDown } from '@element-plus/icons-vue'
import { ElMessage } from 'element-plus'
import { computed, inject, reactive, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import { api } from '~/api'
import { setLocale } from '~/i18n'

const { t, locale } = useI18n()
const router = useRouter()
const markAuthed = inject<() => void>('markAuthed', () => {})
const loading = ref(false)
const form = reactive({
  username: '',
  password: '',
})
const labelWidth = computed(() => (locale.value === 'zh-CN' ? '72px' : '96px'))

function onLocale(command: string) {
  setLocale(command as AppLocale)
}

async function submit() {
  if (!form.username.trim() || !form.password) {
    ElMessage.error(t('login.needFields'))
    return
  }
  loading.value = true
  try {
    await api.login(form)
    markAuthed()
    await router.replace('/')
  }
  catch (e) {
    ElMessage.error((e as Error).message || t('login.failed'))
  }
  finally {
    loading.value = false
  }
}
</script>

<template>
  <div class="login-wrap">
    <el-dropdown class="lang" trigger="click" @command="onLocale">
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
    <el-card class="login-card" shadow="hover">
      <div class="brand">
        {{ t('app.brand') }}
      </div>
      <div class="sub">
        {{ t('app.brandSub') }}
      </div>
      <el-form class="mt-24px" :label-width="labelWidth" label-position="right" @submit.prevent="submit">
        <el-form-item :label="t('login.username')">
          <el-input v-model="form.username" autocomplete="username" @keyup.enter="submit" />
        </el-form-item>
        <el-form-item :label="t('login.password')">
          <el-input v-model="form.password" type="password" show-password autocomplete="current-password" @keyup.enter="submit" />
        </el-form-item>
        <el-form-item>
          <el-button class="w-full" type="primary" :loading="loading" native-type="submit" @click="submit">
            {{ t('login.submit') }}
          </el-button>
        </el-form-item>
      </el-form>
    </el-card>
  </div>
</template>

<style scoped>
.login-wrap {
  position: relative;
  min-height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  background: #0f172a;
}
.lang {
  position: absolute;
  top: 20px;
  right: 24px;
}
.lang-switch {
  display: inline-flex;
  align-items: center;
  color: #cbd5e1;
  font-size: 14px;
  cursor: pointer;
}
.login-card {
  width: 400px;
}
.brand {
  font-size: 28px;
  font-weight: 700;
  color: #0f172a;
}
.sub {
  margin-top: 4px;
  color: #64748b;
  font-size: 13px;
}
</style>
