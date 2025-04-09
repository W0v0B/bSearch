<template>
  <Transition name="fade">
    <div 
      v-show="isVisible" 
      class="search-overlay"
      @click.self="hideSearch"
    >
      <div 
        ref="searchContainer" 
        class="search-container"
        @keydown="handleKeyDown"
      >
        <div class="search-input-wrapper">
          <div class="search-icon">
            <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <circle cx="11" cy="11" r="8"></circle>
              <line x1="21" y1="21" x2="16.65" y2="16.65"></line>
            </svg>
          </div>
          <input 
            ref="searchInput"
            v-model="searchTerm"
            type="text"
            placeholder="搜索应用或输入网址..."
            @input="performSearch"
            class="search-input"
          />
          <div 
            v-if="searchTerm" 
            class="clear-icon" 
            @click="clearSearch"
          >
            <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <line x1="18" y1="6" x2="6" y2="18"></line>
              <line x1="6" y1="6" x2="18" y2="18"></line>
            </svg>
          </div>
        </div>
        
        <div 
          v-if="isLoading" 
          class="loading-indicator"
        >
          <div class="spinner"></div>
          <span>搜索中...</span>
        </div>
        
        <div 
          v-else-if="results.length > 0" 
          class="search-results"
        >
          <div class="result-category" v-if="appResults.length > 0">
            <div class="category-title">应用程序</div>
            <div 
              v-for="(result, index) in appResults" 
              :key="`app-${index}`"
              :class="['result-item', { 'selected': selectedIndex === getAbsoluteIndex(index, 'app') }]"
              @click="executeResult(result)"
              @mouseenter="selectedIndex = getAbsoluteIndex(index, 'app')"
            >
              <div class="result-icon">
                <img :src="result.icon_path || '/app-icon-placeholder.svg'" :alt="result.title">
              </div>
              <div class="result-details">
                <div class="result-title">{{ result.title }}</div>
                <div class="result-path">{{ result.path }}</div>
              </div>
              <div class="result-action">
                <span class="keyboard-shortcut">Enter</span>
              </div>
            </div>
          </div>
          
          <div class="result-category" v-if="webResults.length > 0">
            <div class="category-title">网络搜索</div>
            <div 
              v-for="(result, index) in webResults" 
              :key="`web-${index}`"
              :class="['result-item', { 'selected': selectedIndex === getAbsoluteIndex(index, 'web') }]"
              @click="executeResult(result)"
              @mouseenter="selectedIndex = getAbsoluteIndex(index, 'web')"
            >
              <div class="result-icon">
                <img :src="result.icon_path || '/web-icon-placeholder.svg'" :alt="result.title">
              </div>
              <div class="result-details">
                <div class="result-title">{{ result.title }}</div>
                <div class="result-url">{{ result.url }}</div>
              </div>
              <div class="result-action">
                <span class="keyboard-shortcut">Enter</span>
              </div>
            </div>
          </div>
          
          <div class="search-tips">
            <span><kbd>↑</kbd><kbd>↓</kbd> 选择</span>
            <span><kbd>Enter</kbd> 打开</span>
            <span><kbd>Esc</kbd> 关闭</span>
          </div>
        </div>
        
        <div 
          v-else-if="searchTerm && !isLoading" 
          class="no-results"
        >
          <div class="no-results-icon">
            <svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <circle cx="12" cy="12" r="10"></circle>
              <line x1="12" y1="8" x2="12" y2="12"></line>
              <line x1="12" y1="16" x2="12.01" y2="16"></line>
            </svg>
          </div>
          <div class="no-results-text">未找到结果</div>
          <div class="search-web-suggestion">
            <span>按 <kbd>Enter</kbd> 搜索网络: "{{ searchTerm }}"</span>
            <button @click="searchWeb(searchTerm)" class="search-web-button">
              搜索网络
            </button>
          </div>
        </div>
        
        <div 
          v-else 
          class="start-search"
        >
          <div class="recent-searches" v-if="recentSearches.length > 0">
            <div class="category-title">最近搜索</div>
            <div 
              v-for="(search, index) in recentSearches.slice(0, 5)" 
              :key="`recent-${index}`"
              class="recent-search-item"
              @click="setSearch(search)"
            >
              <div class="recent-search-icon">
                <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                  <circle cx="12" cy="12" r="10"></circle>
                  <polyline points="12 6 12 12 16 14"></polyline>
                </svg>
              </div>
              <span>{{ search }}</span>
            </div>
          </div>
          
          <div class="frequent-apps" v-if="frequentApps.length > 0">
            <div class="category-title">常用应用</div>
            <div class="frequent-apps-grid">
              <div 
                v-for="(app, index) in frequentApps.slice(0, 6)" 
                :key="`frequent-${index}`"
                class="frequent-app-item"
                @click="executeResult(app)"
              >
                <div class="frequent-app-icon">
                  <img :src="app.icon_path || '/app-icon-placeholder.svg'" :alt="app.title">
                </div>
                <div class="frequent-app-name">{{ app.title }}</div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  </Transition>
</template>

<script setup lang="ts">
// 导入必要的Vue组合式API和Tauri API
import { ref, computed, onMounted, onUnmounted, watch, nextTick } from 'vue';
import { invoke } from '@tauri-apps/api/core'; // Tauri核心API，用于与后端通信
import { Window } from "@tauri-apps/api/window" // Tauri窗口管理API

// 创建应用窗口实例
const appWindow = new Window('main');

// 状态管理部分：
// 控制搜索界面的显示状态
const isVisible = ref(false); // 是否显示搜索界面
const searchTerm = ref(''); // 当前搜索词
const results = ref<any[]>([]); // 搜索结果列表
const selectedIndex = ref(0); // 当前选中的结果索引
const isLoading = ref(false); // 加载状态
const searchInput = ref<HTMLInputElement | null>(null); // 搜索输入框引用
const searchContainer = ref<HTMLDivElement | null>(null); // 搜索容器引用
const recentSearches = ref<string[]>([]); // 最近搜索记录
const frequentApps = ref<any[]>([]); // 常用应用列表

// 结果过滤器：
// 根据类型过滤应用搜索结果
const appResults = computed(() => {
  return results.value.filter(r => r.type === 'app');
});
const webResults = computed(() => {
  return results.value.filter(r => r.type === 'web');
});

/**
 * 获取绝对索引（用于跨分类选择）
 * 
 * @param index - 当前分类内的索引
 * @param type - 结果类型 ('app' 或 'web')
 * @returns 跨分类的绝对索引
 */
function getAbsoluteIndex(index: number, type: 'app' | 'web'): number {
  if (type === 'app') {
    return index;
  } else {
    return appResults.value.length + index;
  }
}

/**
 * 执行搜索操作
 * 
 * 处理逻辑：
 * 1. 检查搜索词是否为空
 * 2. 设置加载状态
 * 3. 获取应用搜索结果
 * 4. 生成网络搜索结果
 * 5. 合并结果并重置选中索引
 */
async function performSearch(): Promise<void> {
  // 检查搜索词是否为空
  if (!searchTerm.value.trim()) {
    results.value = [];
    return;
  }
  
  // 设置加载状态
  isLoading.value = true;
  
  try {
    // 获取应用搜索结果
    const apps = await invoke('search_apps', { 
      query: searchTerm.value.trim() 
    }) as any[];
    console.log('Received app:', apps);

    const appsWithIcons = await Promise.all(apps.map(async (app) => {
      try {
        if (app.icon_path) {
          // 获取图标数据
          const iconData = await invoke('get_icon_data', { path: app.icon_path });
          return { ...app, icon_path: iconData };
        }
        return app;
      } catch (e) {
        console.error('Failed to load icon:', e);
        return app;
      }
    }));
    
    // 模拟网络搜索结果
    const webSearchResults = [
      {
        type: 'web',
        title: `搜索 "${searchTerm.value}" - Google`,
        url: `https://www.google.com/search?q=${encodeURIComponent(searchTerm.value)}`,
        icon_path: '/google-icon.svg'
      },
      {
        type: 'web',
        title: `搜索 "${searchTerm.value}" - Bing`,
        url: `https://www.bing.com/search?q=${encodeURIComponent(searchTerm.value)}`,
        icon_path: '/edge-icon.svg'
      }
    ];
    
    // 合并结果
    results.value = [...appsWithIcons, ...webSearchResults];
    selectedIndex.value = 0;
  } catch (error) {
    console.error('搜索失败:', error);
  } finally {
    isLoading.value = false;
  }
}

/**
 * 处理键盘导航事件
 * 
 * 支持的按键：
 * - ArrowDown: 向下选择
 * - ArrowUp: 向上选择
 * - Enter: 执行选中结果
 * - Escape: 关闭搜索界面
 */
function handleKeyDown(event: KeyboardEvent): void {
  switch (event.key) {
    case 'ArrowDown':
      event.preventDefault();
      selectedIndex.value = (selectedIndex.value + 1) % results.value.length;
      break;
    case 'ArrowUp':
      event.preventDefault();
      selectedIndex.value = (selectedIndex.value - 1 + results.value.length) % results.value.length;
      break;
    case 'Enter':
      event.preventDefault();
      if (results.value.length > 0) {
        executeResult(results.value[selectedIndex.value]);
      } else if (searchTerm.value.trim()) {
        searchWeb(searchTerm.value);
      }
      break;
    case 'Escape':
      event.preventDefault();
      hideSearch();
      break;
  }
}

/**
 * 执行选中结果
 * 
 * 处理逻辑：
 * 1. 根据结果类型执行不同操作
 * 2. 添加到最近搜索记录
 * 3. 隐藏搜索界面
 */
async function executeResult(result: any): Promise<void> {
  try {
    switch(result.type) {
      case 'app':
        await invoke('launch_app', { appPath: result.path });
        break;
      case 'web':
        await invoke('open_url', { url: result.url });
        break;
    }
    
    // 成功处理后添加到最近搜索并关闭
    addToRecentSearches(searchTerm.value);
    hideSearch();
  } catch (error) {
    console.error(`${result.type === 'app' ? '启动应用' : '打开URL'}失败:`, error);
  }
}

/**
 * 执行网络搜索
 * 
 * 处理逻辑：
 * 1. 调用Tauri API执行网络搜索
 * 2. 添加到最近搜索记录
 * 3. 隐藏搜索界面
 */
 async function searchWeb(query: string, browser: string = 'google'): Promise<void> {
  try {
    let searchUrl;
    
    // 根据浏览器类型构建URL
    if (browser.toLowerCase() === 'edge') {
      searchUrl = `https://www.bing.com/search?q=${encodeURIComponent(query)}`;
    } else {
      // 默认使用 Google
      searchUrl = `https://www.google.com/search?q=${encodeURIComponent(query)}`;
    }
    
    await invoke('open_url', { url: searchUrl });
    addToRecentSearches(query);
    hideSearch();
  } catch (error) {
    console.error('网络搜索失败:', error);
  }
}

// 清除搜索
function clearSearch() {
  searchTerm.value = '';
  results.value = [];
  searchInput.value?.focus();
}

// 设置搜索词
function setSearch(term: string) {
  searchTerm.value = term;
  performSearch();
}

// 添加到最近搜索
function addToRecentSearches(term: string) {
  if (!term.trim()) return;
  
  // 移除已有的相同搜索词
  recentSearches.value = recentSearches.value.filter(s => s !== term);
  
  // 添加到最前面
  recentSearches.value.unshift(term);
  
  // 限制最大数量
  if (recentSearches.value.length > 10) {
    recentSearches.value = recentSearches.value.slice(0, 10);
  }
  
  // 保存到本地存储
  localStorage.setItem('recentSearches', JSON.stringify(recentSearches.value));
}

// 显示搜索界面
async function showSearch() {
  isVisible.value = true;
  await nextTick();
  searchInput.value?.focus();
  
  // 加载常用应用
  loadFrequentApps();
}

// 隐藏搜索界面
async function hideSearch() {
  isVisible.value = false;
  searchTerm.value = '';
  results.value = [];
  try {
    await invoke('hide_main_window');
    console.log('Window hidden via hideSearch');
  } catch (error) {
    console.error("Failed to hide window:", error);
  }
}

// 加载常用应用
async function loadFrequentApps() {
  try {
    frequentApps.value = await invoke('get_frequent_apps') as any[];
  } catch (error) {
    console.error('加载常用应用失败:', error);
    frequentApps.value = [];
  }
}

// 生命周期钩子
onMounted(() => {
  // 监听全局事件
  window.addEventListener('keydown', handleGlobalKeyDown);
  
  // 从本地存储加载搜索历史
  const savedSearches = localStorage.getItem('recentSearches');
  if (savedSearches) {
    try {
      recentSearches.value = JSON.parse(savedSearches);
    } catch {
      recentSearches.value = [];
    }
  }
  
  // 监听窗口事件
  appWindow.listen('window-shown', () => {
    showSearch();
  });
  
  appWindow.listen('window-hidden', () => {
    hideSearch();
  });
});

onUnmounted(() => {
  window.removeEventListener('keydown', handleGlobalKeyDown);
});

// 全局按键处理
function handleGlobalKeyDown(event: KeyboardEvent) {
  // 处理特殊快捷键
  if (event.shiftKey && event.code === 'Space') {
    event.preventDefault();
    if (isVisible.value) {
      hideSearch();
    } else {
      showSearch();
    }
  }
}

// 监听搜索输入变化
watch(searchTerm, (newVal) => {
  if (newVal.trim()) {
    performSearch();
  } else {
    results.value = [];
  }
});
</script>

<style scoped>
/* 动画效果 */
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.2s ease;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}

/* 搜索界面样式 */
.search-overlay {
  position: fixed;
  top: 0;
  left: 0;
  width: 100%;
  height: 100%;
  background-color: rgba(0, 0, 0, 0.5);
  backdrop-filter: blur(5px);
  display: flex;
  justify-content: center;
  align-items: flex-start;
  padding-top: 120px;
  z-index: 9999;
}

.search-container {
  width: 600px;
  max-width: 90vw;
  background-color: rgba(255, 255, 255, 0.95);
  border-radius: 12px;
  box-shadow: 0 10px 25px rgba(0, 0, 0, 0.2);
  overflow: hidden;
  display: flex;
  flex-direction: column;
}

/* 深色模式支持 */
@media (prefers-color-scheme: dark) {
  .search-container {
    background-color: rgba(30, 30, 30, 0.95);
    color: #f0f0f0;
  }
}

/* 搜索输入框 */
.search-input-wrapper {
  display: flex;
  align-items: center;
  padding: 16px;
  border-bottom: 1px solid rgba(0, 0, 0, 0.1);
}

.search-icon {
  margin-right: 12px;
  color: #666;
}

.search-input {
  flex: 1;
  border: none;
  background: transparent;
  font-size: 18px;
  padding: 8px 0;
  outline: none;
  color: inherit;
}

.clear-icon {
  cursor: pointer;
  color: #999;
  padding: 4px;
  border-radius: 50%;
}

.clear-icon:hover {
  background-color: rgba(0, 0, 0, 0.05);
  color: #666;
}

/* 加载指示器 */
.loading-indicator {
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 20px;
  color: #666;
}

.spinner {
  width: 20px;
  height: 20px;
  border: 2px solid rgba(0, 0, 0, 0.1);
  border-top-color: #3498db;
  border-radius: 50%;
  animation: spin 1s linear infinite;
  margin-right: 10px;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}

/* 搜索结果 */
.search-results {
  overflow-y: auto;
  max-height: 60vh;
}

.result-category {
  padding: 8px 0;
}

.category-title {
  padding: 8px 16px;
  font-size: 12px;
  text-transform: uppercase;
  color: #666;
  font-weight: 600;
}

.result-item {
  display: flex;
  align-items: center;
  padding: 12px 16px;
  cursor: pointer;
  transition: background-color 0.2s;
}

.result-item:hover,
.result-item.selected {
  background-color: rgba(0, 120, 255, 0.1);
}

.result-icon {
  width: 32px;
  height: 32px;
  margin-right: 12px;
  display: flex;
  justify-content: center;
  align-items: center;
}

.result-icon img {
  max-width: 100%;
  max-height: 100%;
}

.result-details {
  flex: 1;
}

.result-title {
  font-weight: 500;
  margin-bottom: 2px;
}

.result-path,
.result-url {
  font-size: 12px;
  color: #666;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 400px;
}

.result-action {
  margin-left: 16px;
}

.keyboard-shortcut {
  background-color: rgba(0, 0, 0, 0.05);
  padding: 2px 6px;
  border-radius: 4px;
  font-size: 12px;
  color: #666;
}

/* 搜索提示 */
.search-tips {
  display: flex;
  justify-content: center;
  gap: 16px;
  padding: 12px;
  border-top: 1px solid rgba(0, 0, 0, 0.1);
  font-size: 12px;
  color: #666;
}

kbd {
  background-color: #f0f0f0;
  border: 1px solid #ccc;
  border-radius: 3px;
  box-shadow: 0 1px 1px rgba(0, 0, 0, 0.2);
  color: #333;
  display: inline-block;
  font-size: 11px;
  line-height: 1;
  padding: 3px 5px;
  margin: 0 2px;
}

@media (prefers-color-scheme: dark) {
  kbd {
    background-color: #333;
    border-color: #444;
    color: #f0f0f0;
  }
}

/* 无结果状态 */
.no-results {
  padding: 32px 16px;
  text-align: center;
}

.no-results-icon {
  margin-bottom: 16px;
  color: #666;
}

.no-results-text {
  font-size: 18px;
  margin-bottom: 16px;
}

.search-web-suggestion {
  margin-top: 16px;
}

.search-web-button {
  margin-top: 12px;
  background-color: #3498db;
  color: white;
  border: none;
  padding: 8px 16px;
  border-radius: 4px;
  cursor: pointer;
  font-size: 14px;
  transition: background-color 0.2s;
}

.search-web-button:hover {
  background-color: #2980b9;
}

/* 初始状态 - 最近搜索和常用应用 */
.start-search {
  padding: 16px;
}

.recent-search-item {
  display: flex;
  align-items: center;
  padding: 8px 12px;
  cursor: pointer;
  border-radius: 6px;
  transition: background-color 0.2s;
}

.recent-search-item:hover {
  background-color: rgba(0, 0, 0, 0.05);
}

.recent-search-icon {
  margin-right: 10px;
  color: #666;
}

.frequent-apps {
  margin-top: 20px;
}

.frequent-apps-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 16px;
  margin-top: 12px;
}

.frequent-app-item {
  display: flex;
  flex-direction: column;
  align-items: center;
  padding: 12px;
  border-radius: 8px;
  cursor: pointer;
  transition: background-color 0.2s;
}

.frequent-app-item:hover {
  background-color: rgba(0, 0, 0, 0.05);
}

.frequent-app-icon {
  width: 48px;
  height: 48px;
  margin-bottom: 8px;
  display: flex;
  justify-content: center;
  align-items: center;
}

.frequent-app-name {
  font-size: 12px;
  text-align: center;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  width: 100%;
}

/* 响应式调整 */
@media (max-width: 640px) {
  .search-overlay {
    padding-top: 80px;
  }
  
  .frequent-apps-grid {
    grid-template-columns: repeat(2, 1fr);
  }
}
</style>
