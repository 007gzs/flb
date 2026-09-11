import ElementPlus from 'element-plus'
import { createApp } from 'vue'
import { createRouter, createWebHistory } from 'vue-router'
import { routes } from 'vue-router/auto-routes'
import App from './App.vue'
import { i18n } from './i18n'
import 'element-plus/dist/index.css'
import 'element-plus/theme-chalk/dark/css-vars.css'
import 'uno.css'
import './styles/index.scss'

const router = createRouter({
  history: createWebHistory(),
  routes,
})

const app = createApp(App)
app.use(i18n)
app.use(router)
app.use(ElementPlus)
app.mount('#app')
