<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted, type Component } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { saveBookmarks, loadBookmarks, exportBookmarks, importBookmarks } from './utils/storage'
import { openUrl } from '@tauri-apps/plugin-opener'
import { check as checkForUpdate } from '@tauri-apps/plugin-updater'
import { relaunch } from '@tauri-apps/plugin-process'
import {
  Search, Plus, Trash2, Pencil, Bookmark, X, Compass, Briefcase, Tv, Wrench,
  ArrowUpRight, FolderPlus, Folder, FileQuestion, SearchX, Sun, Moon, RotateCw,
  AlertTriangle, ArrowUpDown, ImagePlus, GripVertical, Download, Upload
} from 'lucide-vue-next'

interface Category {
  id: number
  name: string
  icon: Component
  isDefault: boolean
}

interface BookmarkItem {
  id: number
  title: string
  url: string
  category: string
  description: string
  icon?: string
}

// ==================== 分类 ====================
const categories = ref<Category[]>([
  { id: 0, name: '全部', icon: Compass, isDefault: true },
  { id: 2, name: '工作/学习', icon: Briefcase, isDefault: false },
  { id: 3, name: '娱乐/媒体', icon: Tv, isDefault: false },
  { id: 4, name: '开发工具', icon: Wrench, isDefault: false },
])

const activeIndex = ref(0)
const currentCategoryName = computed(() => categories.value[activeIndex.value]?.name || '全部')

const newCategoryName = ref('')
const isCategoryModalOpen = ref(false)

const categoryCounts = computed(() => {
  const map: Record<string, number> = { '全部': bookmarks.value.length }
  for (const b of bookmarks.value) {
    map[b.category] = (map[b.category] || 0) + 1
  }
  return map
})
const getCategoryCount = (name: string) => categoryCounts.value[name] || 0

const selectCategory = (index: number) => {
  activeIndex.value = index
}

const handleAddCategory = async () => {
  const trimmed = newCategoryName.value.trim()
  if (!trimmed) return
  if (categories.value.some(c => c.name === trimmed)) {
    showToast('该分类名称已存在')
    return
  }

  categories.value.push({
    id: Date.now(),
    name: trimmed,
    icon: Folder,
    isDefault: false
  })

  newCategoryName.value = ''
  isCategoryModalOpen.value = false

  await saveBookmarks({ categories: categories.value, bookmarks: bookmarks.value })
}

const deleteCategory = (index: number, e: MouseEvent) => {
  e.stopPropagation()
  const target = categories.value[index]
  if (target.isDefault) return

  const count = bookmarks.value.filter(b => b.category === target.name).length
  showConfirmDialog(
    `确定删除分类「${target.name}」吗？`,
    count > 0 ? `该分类下还有 ${count} 个书签，删除分类后书签不会被删除。` : '此操作不可撤销。',
    async () => {
      categories.value.splice(index, 1)
      if (activeIndex.value === index) {
        activeIndex.value = 0
      } else if (activeIndex.value > index) {
        activeIndex.value -= 1
      }
      await saveBookmarks({ categories: categories.value, bookmarks: bookmarks.value })
    }
  )
}

// ---- 分类拖拽排序 ----
const draggedCatIndex = ref<number | null>(null)
const dragOverCatIndex = ref<number | null>(null)

const onCatDragStart = (index: number, e: DragEvent) => {
  if (index === 0) { e.preventDefault(); return }
  draggedCatIndex.value = index
  if (e.dataTransfer) {
    e.dataTransfer.effectAllowed = 'move'
    e.dataTransfer.setData('text/plain', '')
  }
}

const onCatDragOver = (index: number, e: DragEvent) => {
  if (draggedCatIndex.value === null || index === 0) return
  e.preventDefault()
  if (e.dataTransfer) e.dataTransfer.dropEffect = 'move'
  dragOverCatIndex.value = index
}

const onCatDrop = async (index: number, e: DragEvent) => {
  e.preventDefault()
  const fromIndex = draggedCatIndex.value
  if (fromIndex === null || index === 0 || fromIndex === index) {
    draggedCatIndex.value = null
    dragOverCatIndex.value = null
    return
  }

  const selectedId = categories.value[activeIndex.value]?.id
  const arr = [...categories.value]
  const [moved] = arr.splice(fromIndex, 1)
  arr.splice(fromIndex < index ? index - 1 : index, 0, moved)
  categories.value = arr

  const newActive = arr.findIndex(c => c.id === selectedId)
  if (newActive >= 0) activeIndex.value = newActive

  draggedCatIndex.value = null
  dragOverCatIndex.value = null
  await saveBookmarks({ categories: categories.value, bookmarks: bookmarks.value })
}

const onCatDragEnd = () => {
  draggedCatIndex.value = null
  dragOverCatIndex.value = null
}

// ---- 分类重命名（侧边栏内联编辑）----
const editingCategoryId = ref<number | null>(null)
const editingCategoryName = ref('')

const startRenameCategory = (item: Category, e: MouseEvent) => {
  e.stopPropagation()
  editingCategoryId.value = item.id
  editingCategoryName.value = item.name
}

const cancelRenameCategory = () => {
  editingCategoryId.value = null
  editingCategoryName.value = ''
}

const confirmRenameCategory = async (item: Category) => {
  if (editingCategoryId.value !== item.id) return

  const trimmed = editingCategoryName.value.trim()
  cancelRenameCategory()

  if (!trimmed || trimmed === item.name) return
  if (categories.value.some(c => c.id !== item.id && c.name === trimmed)) {
    showToast('该分类名称已存在')
    return
  }

  // 分类和书签之间是靠名字字符串关联的，改名后要同步更新所有归属该分类的书签
  const oldName = item.name
  item.name = trimmed
  bookmarks.value.forEach(b => {
    if (b.category === oldName) b.category = trimmed
  })

  await saveBookmarks({ categories: categories.value, bookmarks: bookmarks.value })
}

// ==================== 书签 ====================
// 首次启动、本地又没有已保存数据时的状态：保持真正的空列表，
// 不要放示例书签进来，否则会被误认成是已经导入的真实数据
const bookmarks = ref<BookmarkItem[]>([])

const currentCategoryBookmarks = computed(() => {
  if (currentCategoryName.value === '全部') return bookmarks.value
  return bookmarks.value.filter(item => item.category === currentCategoryName.value)
})

const searchQuery = ref('')
const sortMode = ref<'default' | 'name' | 'name-desc'>('default')

const cycleSortMode = () => {
  const modes: Array<'default' | 'name' | 'name-desc'> = ['default', 'name', 'name-desc']
  const idx = modes.indexOf(sortMode.value)
  sortMode.value = modes[(idx + 1) % modes.length]
}

const sortLabel = computed(() => {
  if (sortMode.value === 'name') return '名称 A→Z'
  if (sortMode.value === 'name-desc') return '名称 Z→A'
  return '默认排序'
})

const _py: Record<string, string> = {}
const _r = (k: string, s: string) => { for (const c of s) _py[c] = k }
_r('a', '阿啊哀哎唉矮爱碍安暗按昂凹奥澳')
_r('b', '八巴把爸白百柏摆败拜班般斑搬板版办半伴扮瓣帮绑镑棒包薄宝饱保报抱暴爆杯北贝备背倍被辈奔本笔毕闭币必避壁臂边编扁便变遍辩标表别宾冰兵饼并病拨波博勃搏脖泊驳捕补不布步部簿哔')
_r('c', '才材财裁采彩菜蔡参餐残蚕惨灿仓苍藏操草策层曾叉插查茶察差拆柴缠产铲阐颤昌猖场尝常偿肠厂敞畅倡超抄钞朝潮巢吵炒车扯彻撤尘臣沉陈衬趁称城诚承程惩橙成盛乘池迟驰尺齿耻斥赤翅充冲虫崇抽仇绸愁筹酬丑臭出初除厨础储楚处触川穿传船喘串窗床创吹炊垂锤春纯唇蠢促醋粗簇催脆翠村存寸措错搓')
_r('d', '达答搭打大呆代带待袋戴丹单担胆旦但诞弹蛋当挡党荡刀导倒岛蹈到盗道稻德的灯等低底地弟帝递第典点电店垫淀殿碉叼雕凋吊钓调掉跌叠蝶丁叮盯钉顶订定丢东冬董懂动冻洞都斗抖陡豆逗毒独读堵赌杜肚度渡端短段断锻堆队对吨蹲敦盾顿多夺朵垛跺躲惰')
_r('e', '鹅额恶恩儿耳二贰')
_r('f', '发乏伐罚阀法帆番翻凡烦繁反返范贩饭泛坊芳方防妨房纺访放飞非肥废肺费分吩纷坟粉份奋愤粪丰封风疯峰锋蜂冯逢缝凤奉佛否夫肤伏扶服浮符幅福辐蝠府抚辅腐付妇负附赋复赴副富腹覆')
_r('g', '该改盖概干甘竿肝赶敢感刚钢缸港高搞稿告哥歌格隔革个各给根跟耕更工弓公功攻供宫恭巩拱共贡勾沟钩狗够构购估咕姑鼓古股骨谷固故顾瓜刮挂拐怪关观官冠管馆罐灌贯光广逛规归龟闺桂贵跪滚锅国果裹过')
_r('h', '哈孩海害含寒函喊汉汗旱杭航毫豪好号浩耗呵喝合何和河核荷盒贺贵褐鹤恨哼横衡恒轰哄红宏洪虹猴吼后厚候乎呼忽狐胡壶湖葫虎互户护沪花华划化画话怀淮坏欢环还缓换患唤荒慌皇黄煌晃灰恢挥辉回毁悔汇会惠慧毁绘昏婚混活火伙或货获祸惑霍')
_r('j', '击基机积饥迹激及吉级极即急集籍几己计记纪忌技际季继寂寄加佳嘉夹甲价驾架假嫁尖坚间肩艰兼监减剪检简见件建剑健渐践鉴键箭江姜将浆僵疆讲奖桨匠降酱交郊焦胶角脚搅缴叫较教阶皆接揭街节劫杰洁结截竭姐解介戒届界借今斤金津筋仅紧锦尽劲近进晋浸禁京经精睛景警净竟境镜纠究九久酒旧救就居局菊橘举矩句具俱剧据距聚卷倦绢决觉掘嚼军均君菌俊竣加密')
_r('k', '卡开凯刊堪看康扛抗炕考烤靠科棵颗壳咳可渴克刻客课肯坑空孔恐控口扣枯哭苦库裤酷夸垮挎跨块快宽款况矿框旷亏葵愧溃昆困扩括阔')
_r('l', '拉啦垃辣蜡腊来赖兰拦栏蓝篮览懒烂滥朗浪捞劳牢老乐了雷垒泪类冷愣离梨犁璃黎礼李里理力历厉立丽利例隶粒连怜莲联廉帘脸链恋练良凉梁粮两亮辆量谅辽疗聊料列劣猎裂烈林临邻淋灵铃陵岭领令另溜刘流留硫瘤柳六龙聋笼隆垄拢楼漏露炉卢芦鲁陆录鹿碌路驴旅铝屡律率滤绿乱掠略论萝罗逻骡裸落洛络')
_r('m', '妈麻马码蚂骂嘛吗埋买迈麦卖脉蛮满馒瞒漫忙芒茫盲猫毛矛茅冒帽貌贸么没玫枚梅每美妹媒煤霉门闷们萌猛蒙盟孟梦迷谜弥米秘密蜜眠棉免勉面苗描秒妙庙灭民敏名明鸣命摸模膜磨魔末莫墨默谋某母亩木目牧幕墓暮慕穆')
_r('n', '拿哪那纳娜乃奶耐南男难脑恼闹呢内嫩能尼泥你逆年念娘酿鸟尿捏宁拧牛扭纽农浓弄奴努怒女暖诺挪')
_r('o', '偶欧鸥噢哦')
_r('p', '趴爬怕拍排牌派攀盘判叛盼庞旁胖抛跑泡炮陪培赔佩配盆喷朋棚蓬捧碰批披劈皮疲脾匹屁片偏篇骗漂飘票拼贫品聘平评凭苹屏瓶坡泼婆迫破剖扑铺朴普谱瀑曝')
_r('q', '七妻漆齐其奇骑棋旗企启起岂气弃汽泣器千迁牵铅签前钱潜浅遣堑枪腔强墙抢悄敲桥瞧巧切茄且窃侵亲琴勤青轻氢清情晴倾庆穷丘秋蚯求球区曲驱屈趋取去趣圈全权泉拳犬劝缺却雀确裙群')
_r('r', '然燃染嚷让饶绕热人仁忍认任扔仍日绒荣容溶融柔揉肉如乳辱入软锐瑞润弱')
_r('s', '撒洒赛三散桑丧嗓扫嫂色森杀沙纱砂啥晒山删善扇伤商赏上尚裳梢烧稍勺少绍哨舌蛇舍设社射涉摄伸申身深神审婶升生声省胜盛剩圣师失狮施湿十什石识实拾食蚀史使始士世市示事侍释饰视试适是逝誓室收手守首寿受兽售书叔殊梳舒输蔬熟暑鼠属术束述树竖数刷耍摔甩帅双霜爽水谁睡顺瞬说丝司私思斯撕死四寺似松宋送颂搜艘苏俗素速粟塑酸蒜算随虽碎岁穗孙损笋缩所索锁')
_r('t', '他她它塌踏台抬太态贪摊滩坛谈探叹汤唐堂糖躺趟掏涛逃桃淘陶套特疼腾梯踢提题体替天田甜填挑条跳贴铁厅听廷亭停挺通统痛偷投透突图涂途屠土吐兔团推腿退吞托拖脱驼妥拓')
_r('w', '挖蛙哇歪外弯湾丸完玩顽挽晚碗万汪王亡网往忘旺望危威微为围违唯维伟伪尾委味畏卫未位慰温文闻纹稳问翁窝我沃卧握乌污屋无五午伍武舞务物误雾')
_r('x', '西吸希析息牺悉稀惜溪锡膝熙嘻习席袭洗喜细隙虾瞎峡狭下夏吓掀先鲜纤咸闲弦贤衔显险现线宪陷限献乡相香箱详祥享响想象橡项削消萧硝销小晓孝效校笑些歇鞋协携写泄卸谢屑芯辛新信兴星腥刑型形醒幸性姓兄胸雄休修秀绣袖嗅须虚需徐许序叙畜宣悬旋选穴学雪血寻巡旬循训迅讯')
_r('y', '丫压鸦鸭牙芽崖哑雅亚咽烟淹盐严颜阎岩沿演眼衍厌宴艳验焰雁燕央殃秧扬杨洋阳养样腰邀摇遥咬药要耀爷也冶野业叶页夜液一衣医依仪宜姨移遗疑已乙以蚁椅义亿忆议谊译异翼益意毅因阴音吟引饮隐印英婴鹰迎盈营蝇影应映硬哟拥永咏勇涌用优忧尤由油游友有又右幼于鱼余与雨语玉育郁预域遇愈誉渊元园原源远院愿怨冤约月越跃阅云允运蕴酝孕')
_r('z', '杂灾载栽再在咱暂赞脏葬遭糟早枣澡灶造则择泽责贼怎增赠扎眨炸诈摘宅窄寨沾粘展崭占战站张章彰掌涨丈帐账仗障招找召兆赵照罩遮折哲者这针侦珍真诊枕阵振震镇争征挣睁整正证政郑症之支汁织知肢脂蜘执直值职植殖指止纸至志治秩智置中忠终钟种肿众重仲周洲粥州舟轴宙皱骤朱猪竹烛逐主煮嘱住注驻祝著柱筑铸抓爪拽专砖转庄装壮状撞追准捉桌着仔兹资紫字自宗综棕踪总纵走奏租足族阻组嘴最醉罪尊遵昨左做作坐座')

const getSortKey = (title: string): string => {
  const first = title.charAt(0)
  if (/[一-鿿]/.test(first)) {
    return _py[first] || 'z'
  }
  return first.toLowerCase()
}

const filteredBookmarks = computed(() => {
  const q = searchQuery.value.toLowerCase()
  const result = currentCategoryBookmarks.value.filter(item =>
    item.title.toLowerCase().includes(q) || (item.description || '').toLowerCase().includes(q) || item.url.toLowerCase().includes(q)
  )
  if (sortMode.value === 'name') {
    result.sort((a, b) => {
      const ka = getSortKey(a.title), kb = getSortKey(b.title)
      if (ka !== kb) return ka < kb ? -1 : 1
      return a.title.localeCompare(b.title)
    })
  } else if (sortMode.value === 'name-desc') {
    result.sort((a, b) => {
      const ka = getSortKey(a.title), kb = getSortKey(b.title)
      if (ka !== kb) return ka > kb ? -1 : 1
      return b.title.localeCompare(a.title)
    })
  }
  return result
})

const highlightText = (text: string) => {
  const q = searchQuery.value.trim()
  if (!q) return text
  const escaped = q.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
  const regex = new RegExp(`(${escaped})`, 'gi')
  return text.replace(regex, '<mark class="search-highlight">$1</mark>')
}

const getDomain = (url: string) => {
  try {
    const parsed = new URL(/^https?:\/\//i.test(url) ? url : `https://${url}`)
    return parsed.hostname.replace(/^www\./i, '')
  } catch {
    return url.replace(/^https?:\/\//i, '').split('/')[0].replace(/^www\./i, '')
  }
}

const openBookmark = async (url: string) => {
  const targetUrl = /^https?:\/\//i.test(url) ? url : `https://${url}`
  try {
    await openUrl(targetUrl)
  } catch {
    window.open(targetUrl, '_blank')
  }
}

const newBookmark = ref({
  title: '',
  url: '',
  category: '工作/学习',
  description: '',
  customIcon: ''
})
const isModalOpen = ref(false)

// 非 null 时表示当前弹窗处于「编辑」模式，值为被编辑书签的 id
const editingId = ref<number | null>(null)
const isEditing = computed(() => editingId.value !== null)

const openAddModalWithCategory = () => {
  editingId.value = null
  lastFetchedUrl.value = ''
  fetchedFavicon.value = ''
  newBookmark.value = {
    title: '',
    url: '',
    category: currentCategoryName.value === '全部'
      ? (categories.value[1]?.name || '工作/学习')
      : currentCategoryName.value,
    description: '',
    customIcon: ''
  }
  isModalOpen.value = true
}

const openEditModal = (item: BookmarkItem, e: MouseEvent) => {
  e.stopPropagation()
  editingId.value = item.id
  lastFetchedUrl.value = item.url
  fetchedFavicon.value = item.icon || ''
  newBookmark.value = {
    title: item.title,
    url: item.url,
    category: item.category,
    description: item.description,
    customIcon: item.icon || ''
  }
  isModalOpen.value = true
}

const closeBookmarkModal = () => {
  isModalOpen.value = false
  editingId.value = null
}

// ---- URL 失焦时自动抓取标题、描述和图标 ----
const isFetchingMeta = ref(false)
const lastFetchedUrl = ref('')
const fetchedFavicon = ref('')

const handleUrlBlur = async () => {
  const url = newBookmark.value.url.trim()
  if (!url || url === lastFetchedUrl.value) return
  lastFetchedUrl.value = url
  fetchedFavicon.value = ''

  isFetchingMeta.value = true
  try {
    const json = await invoke<string>('fetch_website_meta', { url })
    const meta = JSON.parse(json)
    if (meta.title && !newBookmark.value.title) {
      newBookmark.value.title = meta.title
    }
    if (meta.description && !newBookmark.value.description) {
      newBookmark.value.description = meta.description
    }
    if (meta.favicon) {
      fetchedFavicon.value = meta.favicon
      if (!newBookmark.value.customIcon) {
        newBookmark.value.customIcon = meta.favicon
      }
    }
  } catch (e) {
    console.warn('自动抓取网站信息失败:', e)
  } finally {
    isFetchingMeta.value = false
  }
}

const iconFileRef = ref<HTMLInputElement | null>(null)

const handleIconFile = (e: Event) => {
  const file = (e.target as HTMLInputElement).files?.[0]
  if (!file) return
  const reader = new FileReader()
  reader.onload = () => {
    newBookmark.value.customIcon = reader.result as string
  }
  reader.readAsDataURL(file)
}

const handleSubmitBookmark = async () => {
  if (!newBookmark.value.url) return

  const finalTitle = newBookmark.value.title || getDomain(newBookmark.value.url)
  const finalDescription = newBookmark.value.description

  const icon = newBookmark.value.customIcon.trim() || fetchedFavicon.value || `https://www.google.com/s2/favicons?domain=${getDomain(newBookmark.value.url)}&sz=64`

  if (editingId.value !== null) {
    const target = bookmarks.value.find(item => item.id === editingId.value)
    if (target) {
      target.title = finalTitle
      target.url = newBookmark.value.url
      target.category = newBookmark.value.category
      target.description = finalDescription || ''
      target.icon = icon
      // 清除旧的图标失败状态，让新图标有机会加载
      failedIcons.value.delete(target.id)
      const newSources = { ...iconSources.value }
      delete newSources[target.id]
      iconSources.value = newSources
    }
  } else {
    bookmarks.value.push({
      id: Date.now(),
      title: finalTitle,
      url: newBookmark.value.url,
      category: newBookmark.value.category,
      description: finalDescription || '',
      icon
    })
  }

  closeBookmarkModal()

  await saveBookmarks({ categories: categories.value, bookmarks: bookmarks.value })
}

const deleteBookmark = (id: number, e: MouseEvent) => {
  e.stopPropagation()
  const target = bookmarks.value.find(item => item.id === id)
  if (!target) return

  showConfirmDialog(
    `确定删除书签「${target.title}」吗？`,
    '此操作不可撤销。',
    async () => {
      const index = bookmarks.value.findIndex(item => item.id === id)
      if (index !== -1) {
        bookmarks.value.splice(index, 1)
        await saveBookmarks({ categories: categories.value, bookmarks: bookmarks.value })
      }
    }
  )
}

// ==================== 颜色工具 ====================
const hashString = (str: string) => {
  let hash = 5381
  for (let i = 0; i < str.length; i++) hash = ((hash << 5) + hash + str.charCodeAt(i)) >>> 0
  return hash
}

const colorPalette = [
  { light: { bg: '#e0f2fe', text: '#0369a1' }, dark: { bg: '#0c4a6e30', text: '#38bdf8' } }, // sky
  { light: { bg: '#d1fae5', text: '#047857' }, dark: { bg: '#064e3b30', text: '#34d399' } }, // emerald
  { light: { bg: '#ede9fe', text: '#6d28d9' }, dark: { bg: '#4c1d9530', text: '#a78bfa' } }, // violet
  { light: { bg: '#fef3c7', text: '#b45309' }, dark: { bg: '#78350f30', text: '#fbbf24' } }, // amber
  { light: { bg: '#ffe4e6', text: '#be123c' }, dark: { bg: '#88133730', text: '#fb7185' } }, // rose
  { light: { bg: '#e0e7ff', text: '#4338ca' }, dark: { bg: '#312e8130', text: '#818cf8' } }, // indigo
  { light: { bg: '#ccfbf1', text: '#0f766e' }, dark: { bg: '#134e4a30', text: '#2dd4bf' } }, // teal
  { light: { bg: '#ffedd5', text: '#c2410c' }, dark: { bg: '#7c2d1230', text: '#fb923c' } }, // orange
]

const getAvatarStyle = (domain: string) => {
  const slot = colorPalette[hashString(domain) % colorPalette.length]
  const c = isDarkMode.value ? slot.dark : slot.light
  return { background: c.bg, color: c.text }
}

const getAvatarLetter = (title: string) => {
  return (title || '?').charAt(0).toUpperCase()
}

const getCategoryTagStyle = (category: string) => {
  const slot = colorPalette[hashString(category) % colorPalette.length]
  if (isDarkMode.value) {
    return { background: slot.dark.bg, color: slot.dark.text, borderColor: slot.dark.text + '30' }
  }
  return { background: slot.light.bg, color: slot.light.text, borderColor: slot.light.text + '30' }
}

// ==================== 图标：多源降级 + 站点自定义 ====================
const iconSources = ref<Record<number, number>>({})
const failedIcons = ref(new Set<number>())

// 域名 -> 自定义图标；值为 'custom:文字' 时全部源失败后显示文字，其他值视为直接 URL
const domainIconMap: Record<string, string> = {
  'qtcn.4c1p0.com': 'custom:98',
}

const getCustomLabel = (domain: string) => {
  const v = domainIconMap[domain]
  if (v?.startsWith('custom:')) return v.slice(7)
  return null
}

const handleIconError = (id: number) => {
  const currentSource = iconSources.value[id] || 0
  if (currentSource < 3) {
    iconSources.value[id] = currentSource + 1
  } else {
    failedIcons.value.add(id)
  }
}

const getIconUrl = (item: BookmarkItem) => {
  const domain = getDomain(item.url)
  const mapped = domainIconMap[domain]
  if (mapped && !mapped.startsWith('custom:')) return mapped

  const sourceIndex = iconSources.value[item.id] || 0
  // source 0: 书签自带的 icon（来自 fetch_website_meta 或 Google API）
  if (sourceIndex === 0 && item.icon) return item.icon
  // source 1: Google favicon API
  if (sourceIndex <= 1) return `https://www.google.com/s2/favicons?domain=${domain}&sz=64`
  // source 2: DuckDuckGo favicon API
  if (sourceIndex === 2) return `https://icons.duckduckgo.com/ip3/${domain}.ico`
  // source 3: 直接从网站根目录获取 /favicon.ico
  const base = /^https?:\/\//i.test(item.url) ? item.url : `https://${item.url}`
  try { return new URL('/favicon.ico', base).href } catch { return `https://${domain}/favicon.ico` }
}

// 刷新缓存后追加时间戳参数，绕过浏览器对图标图片的 HTTP 缓存，强制重新请求
const refreshToken = ref(0)
const withCacheBust = (url: string) => {
  if (!refreshToken.value || url.startsWith('data:')) return url
  return `${url}${url.includes('?') ? '&' : '?'}_r=${refreshToken.value}`
}

// ==================== 删除确认弹窗 ====================
const confirmDialog = ref<{ title: string; message: string; onConfirm: () => void } | null>(null)

const showConfirmDialog = (title: string, message: string, onConfirm: () => void) => {
  confirmDialog.value = { title, message, onConfirm }
}

const handleConfirm = () => {
  confirmDialog.value?.onConfirm()
  confirmDialog.value = null
}

// ==================== Toast 提示 ====================
const toastMessage = ref('')
const toastTimer = ref<ReturnType<typeof setTimeout> | null>(null)

const showToast = (message: string) => {
  toastMessage.value = message
  if (toastTimer.value) clearTimeout(toastTimer.value)
  toastTimer.value = setTimeout(() => { toastMessage.value = '' }, 2500)
}

// ==================== 界面状态 ====================
const isDarkMode = ref(localStorage.getItem('bookmark-nav-dark') === 'true')
const toggleDarkMode = () => {
  isDarkMode.value = !isDarkMode.value
  localStorage.setItem('bookmark-nav-dark', String(isDarkMode.value))
}

const isWindowVisible = ref(!document.hidden)
const handleVisibilityChange = () => { isWindowVisible.value = !document.hidden }

const contentFading = ref(false)
watch(currentCategoryName, () => {
  contentFading.value = true
  setTimeout(() => { contentFading.value = false }, 180)
})

// ==================== 导出 / 导入 ====================
const isExporting = ref(false)
const handleExport = async () => {
  if (isExporting.value) return
  isExporting.value = true
  try {
    const saved = await exportBookmarks()
    if (saved) showToast('已导出书签数据')
  } catch (e) {
    showToast('导出失败: ' + e)
  } finally {
    isExporting.value = false
  }
}

const isImporting = ref(false)
const handleImport = () => {
  if (isImporting.value) return
  showConfirmDialog(
    '确定要导入吗？',
    '导入的数据将整体替换当前的书签和分类（当前数据会自动备份一份，可以从系统托盘找到应用数据目录恢复）。',
    async () => {
      isImporting.value = true
      try {
        const imported = await importBookmarks()
        if (!imported) return // 用户取消了选择文件
        categories.value = (imported.categories || []).map(cat => ({
          ...cat,
          icon: cat.name === '全部' ? Compass : Folder,
          isDefault: cat.name === '全部'
        }))
        bookmarks.value = imported.bookmarks || []
        activeIndex.value = 0
        showToast('导入成功')
      } catch (e) {
        showToast('导入失败: ' + e)
      } finally {
        isImporting.value = false
      }
    }
  )
}

const isRefreshing = ref(false)
const refreshCache = async () => {
  if (isRefreshing.value) return
  isRefreshing.value = true

  try {
    // 图标：清空失败/降级记录，并换一个缓存戳强制浏览器重新请求图标图片
    failedIcons.value = new Set()
    iconSources.value = {}
    refreshToken.value = Date.now()

    // 描述：把还没抓到有效描述的书签重新请求一遍
    const targets = bookmarks.value.filter(b => !b.description || b.description === '暂无描述信息...')
    const fetchDescriptions = async () => {
      const concurrency = 5
      for (let i = 0; i < targets.length; i += concurrency) {
        const batch = targets.slice(i, i + concurrency)
        await Promise.all(batch.map(async (b) => {
          try {
            const desc = await invoke<string>('fetch_website_description', { url: b.url })
            if (desc && desc !== '暂无描述信息...') b.description = desc
          } catch (e) {
            console.warn(`刷新描述失败 (${b.title}):`, e)
          }
        }))
      }
      if (targets.length > 0) {
        await saveBookmarks({ categories: categories.value, bookmarks: bookmarks.value })
      }
    }

    // 大多数情况下没有需要补抓的描述，上面全程没有任何 await 真正挂起，
    // 会导致 isRefreshing 在 Vue 还没来得及渲染出“转圈”状态前就已经复位——
    // 这里保证按钮至少转一会儿，用户才能感知到点击确实生效了
    const minSpin = new Promise(resolve => setTimeout(resolve, 600))
    await Promise.all([fetchDescriptions(), minSpin])
  } finally {
    isRefreshing.value = false
  }
}

// ==================== 全局键盘快捷键 ====================
const searchInputRef = ref<HTMLInputElement | null>(null)

const handleGlobalKeydown = (e: KeyboardEvent) => {
  if (e.key === 'Escape') {
    if (confirmDialog.value) {
      confirmDialog.value = null
    } else if (isModalOpen.value) {
      closeBookmarkModal()
    } else if (isCategoryModalOpen.value) {
      isCategoryModalOpen.value = false
    } else if (editingCategoryId.value !== null) {
      cancelRenameCategory()
    } else if (searchQuery.value) {
      searchQuery.value = ''
    }
    return
  }

  if (e.ctrlKey && e.key === 'f') {
    e.preventDefault()
    searchInputRef.value?.focus()
    return
  }

}

onMounted(() => {
  window.addEventListener('keydown', handleGlobalKeydown)
  document.addEventListener('visibilitychange', handleVisibilityChange)
})
onUnmounted(() => {
  window.removeEventListener('keydown', handleGlobalKeydown)
  document.removeEventListener('visibilitychange', handleVisibilityChange)
})

// ==================== 启动时读取本地数据 ====================
onMounted(async () => {
  const savedData = await loadBookmarks()
  if (!savedData) return

  if (savedData.bookmarks && savedData.bookmarks.length > 0) {
    bookmarks.value = savedData.bookmarks
  }

  if (savedData.categories && savedData.categories.length > 0) {
    categories.value = savedData.categories.map(cat => ({
      ...cat,
      icon: cat.name === '全部' ? Compass : Folder,
      isDefault: cat.name === '全部'
    }))
    // 恢复后统一停在“全部”，避免侧边栏高亮和数据对不上
    activeIndex.value = 0
  }
})

// ==================== 检查更新 ====================
onMounted(async () => {
  try {
    const update = await checkForUpdate()
    if (!update) return
    showConfirmDialog(
      `发现新版本 ${update.version}`,
      update.body || '是否现在下载并安装？安装完成后应用会自动重启。',
      async () => {
        try {
          showToast('正在下载更新...')
          await update.downloadAndInstall()
          await relaunch()
        } catch (e) {
          showToast('更新失败: ' + e)
        }
      }
    )
  } catch (e) {
    // 检查更新失败（比如断网）不影响正常使用，只在控制台记录
    console.warn('检查更新失败:', e)
  }
})
</script>

<template>
  <div :class="['custom-app-container relative flex h-screen text-slate-900 antialiased overflow-hidden selection:bg-sky-500 selection:text-white transition-colors duration-300', isDarkMode ? 'bg-slate-950 text-slate-100' : 'bg-slate-100/70']">

    <!-- 背景多维流光效果 -->
    <div :class="['absolute -top-40 left-10 w-[650px] h-[650px] rounded-full blur-[160px] pointer-events-none will-change-transform', isDarkMode ? 'bg-sky-900/15' : 'bg-sky-300/30', isWindowVisible ? 'animate-pulse-slow' : 'animate-none']"></div>
    <div :class="['absolute top-1/4 -right-20 w-[600px] h-[600px] rounded-full blur-[180px] pointer-events-none will-change-transform', isDarkMode ? 'bg-indigo-950/20' : 'bg-indigo-200/40', isWindowVisible ? 'animate-pulse-slower' : 'animate-none']"></div>

    <!-- 左侧边栏 -->
    <aside :class="['relative z-10 w-72 border-r px-4 py-6 pb-8 flex flex-col justify-between flex-shrink-0 shadow-[4px_0_24px_rgba(0,0,0,0.03)] backdrop-blur-2xl transition-colors duration-300', isDarkMode ? 'bg-slate-900/80 border-slate-800/80' : 'bg-white/70 border-white/80']">
      <div class="flex flex-col h-full min-h-0">
        <!-- 品牌头部区域（优化上下呼吸留白） -->
        <div class="flex items-center gap-3.5 px-2 py-3 mb-6 flex-shrink-0">
          <div class="p-2.5 bg-gradient-to-tr from-sky-400 to-indigo-500 rounded-2xl shadow-lg shadow-sky-500/25 text-white ring-1 ring-white/30">
            <Bookmark class="w-5 h-5" />
          </div>
          <div>
            <h1 class="font-extrabold text-base tracking-tight bg-gradient-to-r from-sky-500 via-indigo-500 to-sky-400 bg-clip-text text-transparent">书签导航</h1>
            <p class="text-xs text-sky-500 font-bold tracking-wider uppercase">Clear Glass</p>
          </div>
        </div>

        <!-- 导航列表区域 -->
        <nav :class="['relative flex-1 overflow-y-auto pr-1 space-y-1.5 custom-scrollbar', isDarkMode ? 'dark-scrollbar' : '']">
          <div
            :class="['absolute left-0 w-full h-[48px] rounded-2xl transition-all duration-300 ease-[cubic-bezier(0.4,0,0.2,1)] pointer-events-none', isDarkMode ? 'bg-slate-800/90 border border-slate-700/80 shadow-[0_4px_20px_rgba(0,0,0,0.4)]' : 'bg-white border border-white/80 shadow-[0_4px_20px_rgba(31,38,135,0.08)] backdrop-blur-md']"
            :style="{ transform: `translateY(${activeIndex * 54}px)` }"
          ></div>

          <div
            v-for="(item, index) in categories"
            :key="item.id"
            @click="selectCategory(index)"
            :draggable="!item.isDefault"
            @dragstart="onCatDragStart(index, $event)"
            @dragover="onCatDragOver(index, $event)"
            @drop="onCatDrop(index, $event)"
            @dragend="onCatDragEnd"
            :class="[
              'group/cat relative z-10 w-full h-[48px] flex items-center justify-between px-4 rounded-2xl text-[15px] font-medium transition-all duration-200 cursor-pointer',
              activeIndex === index
                ? (isDarkMode ? 'text-sky-400 font-bold' : 'text-sky-600 font-bold')
                : getCategoryCount(item.name) === 0
                  ? (isDarkMode ? 'text-slate-600 hover:text-slate-400 hover:bg-slate-800/40' : 'text-slate-400 hover:text-slate-600 hover:bg-white/50')
                  : (isDarkMode ? 'text-slate-400 hover:text-slate-200 hover:bg-slate-800/40' : 'text-slate-600 hover:text-slate-900 hover:bg-white/50'),
              draggedCatIndex === index ? 'opacity-30' : '',
              dragOverCatIndex === index && draggedCatIndex !== null && draggedCatIndex !== index ? 'cat-drop-target' : ''
            ]"
          >
            <div class="flex items-center gap-2.5 min-w-0 flex-1">
              <GripVertical
                v-if="!item.isDefault"
                :class="[
                  'w-3.5 h-3.5 flex-shrink-0 opacity-0 group-hover/cat:opacity-40 hover:!opacity-80 cursor-grab active:cursor-grabbing transition-opacity duration-200 -ml-1 -mr-2',
                  isDarkMode ? 'text-slate-400' : 'text-slate-500'
                ]"
              />
              <component
                :is="item.icon"
                :class="[
                  'w-4 h-4 flex-shrink-0 transition-all duration-300',
                  activeIndex === index ? 'text-sky-500 drop-shadow-[0_0_8px_rgba(56,189,248,0.6)] scale-110'
                    : getCategoryCount(item.name) === 0 ? 'opacity-40' : ''
                ]"
              />
              <input
                v-if="editingCategoryId === item.id"
                v-model="editingCategoryName"
                type="text"
                autofocus
                @click.stop
                @keydown.enter="confirmRenameCategory(item)"
                @keydown.esc="cancelRenameCategory"
                @blur="confirmRenameCategory(item)"
                :class="['w-full min-w-0 bg-transparent border-b outline-none text-[15px] font-medium', isDarkMode ? 'border-sky-500 text-slate-100' : 'border-sky-500 text-slate-900']"
              />
              <span v-else class="truncate" :title="item.name">{{ item.name }}</span>
            </div>

            <span
              v-if="getCategoryCount(item.name) > 0"
              :class="[
                'text-xs font-mono min-w-[22px] text-center px-1.5 py-0.5 rounded-full flex-shrink-0 transition-all duration-200',
                activeIndex === index
                  ? (isDarkMode ? 'bg-sky-950/60 text-sky-400' : 'bg-sky-100/80 text-sky-600')
                  : (isDarkMode ? 'bg-slate-800/60 text-slate-500' : 'bg-slate-100 text-slate-400'),
                !item.isDefault ? 'group-hover/cat:opacity-0 group-hover/cat:w-0 group-hover/cat:min-w-0 group-hover/cat:px-0 group-hover/cat:overflow-hidden' : ''
              ]"
            >{{ getCategoryCount(item.name) }}</span>

            <div v-if="!item.isDefault" class="flex items-center flex-shrink-0">
              <button
                @click="startRenameCategory(item, $event)"
                :class="['opacity-0 group-hover/cat:opacity-100 p-1 text-slate-400 hover:text-sky-500 rounded-lg transition-all duration-200 cursor-pointer', isDarkMode ? 'hover:bg-sky-950/40' : 'hover:bg-sky-50/80']"
                title="重命名分类"
              >
                <Pencil class="w-3.5 h-3.5" />
              </button>
              <button
                @click="deleteCategory(index, $event)"
                :class="['opacity-0 group-hover/cat:opacity-100 p-1 text-slate-400 hover:text-red-500 rounded-lg transition-all duration-200 cursor-pointer', isDarkMode ? 'hover:bg-red-950/40' : 'hover:bg-red-50/80']"
                title="删除分类"
              >
                <Trash2 class="w-3.5 h-3.5" />
              </button>
            </div>
          </div>
        </nav>
      </div>

      <!-- 底部添加分类按钮 -->
      <div :class="['pt-4 border-t flex-shrink-0 mt-2', isDarkMode ? 'border-slate-800' : 'border-slate-200/60']">
        <button
          @click="isCategoryModalOpen = true"
          :class="['w-full flex items-center justify-center gap-2 py-3 rounded-2xl font-medium text-sm transition-all border border-dashed cursor-pointer', isDarkMode ? 'bg-slate-800/40 hover:bg-slate-800/80 text-slate-300 hover:text-sky-400 border-slate-700 hover:border-sky-500/50' : 'bg-white/50 hover:bg-white text-slate-600 hover:text-sky-600 border-slate-300 hover:border-sky-400']"
        >
          <FolderPlus class="w-4 h-4 text-sky-500" />
          <span>添加新分类</span>
        </button>
      </div>
    </aside>

    <!-- 右侧主体内容 -->
    <main class="relative z-10 flex-1 flex flex-col min-w-0">
      <header :class="['h-20 px-10 border-b flex items-center justify-between flex-shrink-0 backdrop-blur-xl shadow-[0_1px_10px_rgba(0,0,0,0.01)] transition-colors duration-300', isDarkMode ? 'bg-slate-900/80 border-slate-800' : 'bg-white/70 border-white/80']">
        <div class="flex items-center gap-3.5">
          <h2 :class="['text-xl font-extrabold tracking-tight', isDarkMode ? 'text-slate-100' : 'text-slate-900']">{{ currentCategoryName }}</h2>
          <span :class="['text-xs font-mono px-2.5 py-0.5 border rounded-full font-bold shadow-2xs', isDarkMode ? 'text-sky-400 bg-sky-950 border-sky-800' : 'text-sky-600 bg-sky-50 border-sky-200/80']">
            {{ filteredBookmarks.length }}
          </span>
          <span :class="['hidden md:inline-block text-sm font-normal', isDarkMode ? 'text-slate-400' : 'text-slate-500']">
            {{ currentCategoryName === '全部' ? '— 所有已收录书签' : `— 当前分类书签` }}
          </span>
        </div>

        <!-- 优化搜索与操作区对齐组合 -->
        <div class="flex items-center gap-3">
          <!-- 搜索框 -->
          <div class="relative w-64 focus-within:w-76 group flex items-center transition-all duration-300">
            <Search class="absolute left-4 top-1/2 -translate-y-1/2 w-4 h-4 text-slate-400 group-focus-within:text-sky-500 transition-colors pointer-events-none" />
            <input
              ref="searchInputRef"
              v-model="searchQuery"
              type="text"
              placeholder="搜索书签..."
              :class="['w-full pl-10 py-2.5 border rounded-2xl text-sm transition-all shadow-sm focus:outline-none focus:ring-4 focus:ring-sky-500/15', searchQuery ? 'pr-9' : 'pr-4', isDarkMode ? 'bg-slate-800 border-slate-700 text-slate-200 placeholder-slate-400 focus:bg-slate-800 focus:border-sky-500' : 'bg-white/90 border-slate-200/80 text-slate-800 placeholder-slate-400 focus:bg-white focus:border-sky-400']"
            />
            <button
              v-if="searchQuery"
              @click="searchQuery = ''"
              :class="['absolute right-3 top-1/2 -translate-y-1/2 p-0.5 rounded-full transition-colors cursor-pointer', isDarkMode ? 'text-slate-400 hover:text-slate-200 hover:bg-slate-700' : 'text-slate-400 hover:text-slate-600 hover:bg-slate-200']"
            >
              <X class="w-3.5 h-3.5" />
            </button>
          </div>

          <!-- 排序按钮 -->
          <button
            @click="cycleSortMode"
            :class="['p-2.5 rounded-2xl border transition-all duration-300 cursor-pointer shadow-2xs active:scale-95 flex items-center gap-1.5', sortMode !== 'default' ? (isDarkMode ? 'bg-sky-950/40 border-sky-800 text-sky-400' : 'bg-sky-50 border-sky-200 text-sky-600') : (isDarkMode ? 'bg-slate-800 border-slate-700 text-slate-300 hover:text-sky-400 hover:bg-slate-800/90' : 'bg-white/90 border-slate-200/80 text-slate-600 hover:text-sky-600 hover:bg-white')]"
            :title="sortLabel"
          >
            <ArrowUpDown class="w-4 h-4" />
            <span v-if="sortMode !== 'default'" class="text-xs font-medium pr-0.5">{{ sortLabel }}</span>
          </button>

          <!-- 导出按钮 -->
          <button
            @click="handleExport"
            :disabled="isExporting"
            :class="['p-2.5 rounded-2xl border transition-all duration-300 cursor-pointer shadow-2xs active:scale-95 disabled:cursor-not-allowed disabled:opacity-60', isDarkMode ? 'bg-slate-800 border-slate-700 text-slate-300 hover:text-sky-400 hover:bg-slate-800/90' : 'bg-white/90 border-slate-200/80 text-slate-600 hover:text-sky-600 hover:bg-white']"
            title="导出书签数据到文件"
          >
            <Download class="w-4 h-4" />
          </button>

          <!-- 导入按钮 -->
          <button
            @click="handleImport"
            :disabled="isImporting"
            :class="['p-2.5 rounded-2xl border transition-all duration-300 cursor-pointer shadow-2xs active:scale-95 disabled:cursor-not-allowed disabled:opacity-60', isDarkMode ? 'bg-slate-800 border-slate-700 text-slate-300 hover:text-sky-400 hover:bg-slate-800/90' : 'bg-white/90 border-slate-200/80 text-slate-600 hover:text-sky-600 hover:bg-white']"
            title="从文件导入书签数据（会替换当前数据）"
          >
            <Upload class="w-4 h-4" />
          </button>

          <!-- 刷新缓存按钮 -->
          <button
            @click="refreshCache"
            :disabled="isRefreshing"
            :class="['p-2.5 rounded-2xl border transition-all duration-300 cursor-pointer shadow-2xs active:scale-95 disabled:cursor-not-allowed disabled:opacity-60', isDarkMode ? 'bg-slate-800 border-slate-700 text-slate-300 hover:text-sky-400 hover:bg-slate-800/90' : 'bg-white/90 border-slate-200/80 text-slate-600 hover:text-sky-600 hover:bg-white']"
            title="刷新图标和描述缓存"
          >
            <RotateCw :class="['w-4 h-4 transition-transform duration-700', isRefreshing ? 'animate-spin' : '']" />
          </button>

          <!-- 亮暗模式切换按钮 -->
          <button
            @click="toggleDarkMode"
            :class="['p-2.5 rounded-2xl border transition-all duration-300 cursor-pointer shadow-2xs active:scale-95', isDarkMode ? 'bg-slate-800 border-slate-700 text-amber-400 hover:bg-slate-800/90' : 'bg-white/90 border-slate-200/80 text-slate-600 hover:text-sky-600 hover:bg-white']"
            :title="isDarkMode ? '切换为浅色模式' : '切换为深色模式'"
          >
            <Sun v-if="isDarkMode" class="w-4 h-4" />
            <Moon v-else class="w-4 h-4" />
          </button>
        </div>
      </header>

      <!-- 卡片网格区域 -->
      <section :class="['flex-1 px-10 py-10 overflow-y-auto custom-scrollbar transition-all duration-200', isDarkMode ? 'dark-scrollbar' : '', contentFading ? 'opacity-0 translate-y-1.5' : 'opacity-100 translate-y-0']">
        <TransitionGroup
          v-if="filteredBookmarks.length > 0"
          name="card-list"
          tag="div"
          class="flex flex-wrap gap-5 w-full relative z-10 items-start"
        >
          <div
            v-for="(item, index) in filteredBookmarks"
            :key="item.id"
            @click="openBookmark(item.url)"
            :style="{ '--stagger': Math.min(index * 40, 300) + 'ms' }"
            :class="['group relative w-[370px] flex-shrink-0 min-h-[190px] border p-6 rounded-3xl transition-all duration-300 hover:-translate-y-1.5 active:scale-[0.97] active:translate-y-0 flex flex-col justify-between cursor-pointer', isDarkMode ? 'bg-slate-800/95 hover:bg-slate-800 border-slate-700/80 hover:border-sky-500/60 shadow-[0_10px_35px_rgba(0,0,0,0.2)] hover:shadow-[0_22px_50px_-12px_rgba(56,189,248,0.15)]' : 'bg-white/95 hover:bg-white border-slate-200/70 hover:border-sky-300 shadow-[0_10px_30px_rgb(0,0,0,0.04)] hover:shadow-[0_20px_45px_-12px_rgba(56,189,248,0.2)]']"
          >
            <div>
              <div class="flex items-center justify-between mb-3">
                <div class="flex items-center gap-3.5 min-w-0 pr-2">
                  <div :class="['w-10 h-10 rounded-2xl border shadow-xs flex items-center justify-center p-1 flex-shrink-0 group-hover:scale-110 group-hover:rotate-3 transition-all duration-300', isDarkMode ? 'bg-slate-800 border-slate-700 group-hover:border-sky-500/50 group-hover:shadow-sky-500/20 group-hover:shadow-md' : 'bg-gradient-to-b from-white to-slate-50 border-slate-200/80 group-hover:border-sky-200 group-hover:shadow-sky-200/40 group-hover:shadow-md']">

                    <img
                      v-if="!failedIcons.has(item.id)"
                      :src="withCacheBust(getIconUrl(item))"
                      class="w-full h-full object-contain rounded-lg"
                      @error="handleIconError(item.id)"
                    />

                    <span v-else-if="getCustomLabel(getDomain(item.url))" class="text-xs font-extrabold text-sky-500 tracking-tighter">{{ getCustomLabel(getDomain(item.url)) }}</span>

                    <span v-else class="w-full h-full rounded-lg flex items-center justify-center text-xs font-bold" :style="getAvatarStyle(getDomain(item.url))">{{ getAvatarLetter(item.title) }}</span>
                  </div>

                  <h3 :class="['font-bold text-base tracking-tight transition-colors truncate', isDarkMode ? 'text-slate-100 group-hover:text-sky-400' : 'text-slate-900 group-hover:text-sky-600']" v-html="highlightText(item.title)">
                  </h3>
                </div>

                <div class="flex items-center gap-1.5 flex-shrink-0">
                  <button
                    @click="openEditModal(item, $event)"
                    :class="['opacity-0 group-hover:opacity-100 translate-y-1 group-hover:translate-y-0 p-1.5 text-slate-400 hover:text-sky-500 rounded-xl transition-all duration-300 cursor-pointer', isDarkMode ? 'hover:bg-sky-950/40' : 'hover:bg-sky-50/80']"
                    title="编辑"
                  >
                    <Pencil class="w-4 h-4" />
                  </button>
                  <button
                    @click="deleteBookmark(item.id, $event)"
                    :class="['opacity-0 group-hover:opacity-100 translate-y-1 group-hover:translate-y-0 p-1.5 text-slate-400 hover:text-red-500 rounded-xl transition-all duration-300 cursor-pointer', isDarkMode ? 'hover:bg-red-950/40' : 'hover:bg-red-50/80']"
                    title="删除"
                  >
                    <Trash2 class="w-4 h-4" />
                  </button>
                  <div :class="['w-8 h-8 rounded-xl border shadow-2xs flex items-center justify-center text-slate-400 group-hover:text-sky-500 group-hover:scale-105 transition-all duration-300', isDarkMode ? 'bg-slate-800/70 border-slate-700 group-hover:bg-sky-950/40 group-hover:border-sky-800' : 'bg-slate-50 border-slate-200/80 group-hover:bg-sky-50 group-hover:border-sky-100']">
                    <ArrowUpRight class="w-4 h-4" />
                  </div>
                </div>
              </div>

              <p v-if="item.description && item.description !== '暂无描述信息...'" :class="['text-[13px] font-normal leading-relaxed line-clamp-2 mb-4', isDarkMode ? 'text-slate-400' : 'text-slate-600']" v-html="highlightText(item.description)">
              </p>
            </div>

            <div :class="['pt-3 border-t flex items-center justify-between text-xs', isDarkMode ? 'border-slate-700/60' : 'border-slate-100']">
                <div class="flex items-center gap-2 text-slate-400 font-mono" :title="item.url">
                  <span class="truncate max-w-[170px]">{{ getDomain(item.url) }}</span>
                </div>
                <span
                  v-if="currentCategoryName === '全部' || searchQuery"
                  class="px-2.5 py-0.5 border rounded-full font-medium text-[11px] shadow-2xs"
                  :style="getCategoryTagStyle(item.category)"
                >
                  {{ item.category }}
                </span>
            </div>
          </div>

          <!-- 添加卡片 -->
          <div
            key="add-new"
            @click="openAddModalWithCategory"
            :style="{ '--stagger': Math.min(filteredBookmarks.length * 40, 300) + 'ms' }"
            :class="['group relative w-[370px] flex-shrink-0 min-h-[190px] border-2 border-dashed p-6 rounded-3xl transition-all duration-300 hover:-translate-y-1 flex flex-col items-center justify-center text-center cursor-pointer', isDarkMode ? 'border-slate-700 hover:border-sky-500 hover:bg-slate-800/40' : 'border-slate-300 hover:border-sky-400 hover:bg-white/50']"
          >
            <div :class="['w-11 h-11 rounded-2xl border flex items-center justify-center text-slate-400 group-hover:text-sky-500 group-hover:scale-110 group-hover:shadow-md group-hover:shadow-sky-500/10 transition-all duration-300 mb-2.5', isDarkMode ? 'bg-slate-800 border-slate-700' : 'bg-white border-slate-200/80']">
              <Plus class="w-5 h-5" />
            </div>
            <h4 :class="['font-bold text-sm transition-colors mb-1', isDarkMode ? 'text-slate-300 group-hover:text-sky-400' : 'text-slate-800 group-hover:text-sky-600']">添加新书签</h4>
            <p class="text-[11px] text-slate-400">在此分类下收录常用网站</p>
          </div>
        </TransitionGroup>

        <!-- 状态 A：分类下没有任何卡片 -->
        <div
          v-else-if="currentCategoryBookmarks.length === 0"
          class="flex flex-col items-center justify-center h-[calc(100%-40px)] text-center py-20"
        >
          <div :class="['w-16 h-16 rounded-3xl border shadow-sm flex items-center justify-center text-sky-500 mb-4 backdrop-blur-md animate-float', isDarkMode ? 'bg-slate-800 border-slate-700' : 'bg-white border-slate-200/80']">
            <FileQuestion class="w-8 h-8" />
          </div>
          <h3 :class="['text-base font-bold mb-1', isDarkMode ? 'text-slate-200' : 'text-slate-900']">暂无收录内容</h3>
          <p class="text-xs text-slate-500 mb-6 max-w-xs">当前「{{ currentCategoryName }}」分类下还没有添加任何书签，快去收录第一个网站吧</p>
          <button
            @click="openAddModalWithCategory"
            class="px-5 py-2.5 bg-sky-500 hover:bg-sky-600 text-white rounded-xl text-xs font-semibold transition-all shadow-md shadow-sky-500/20 active:scale-95 flex items-center gap-2 cursor-pointer"
          >
            <Plus class="w-4 h-4" />
            <span>立即添加书签</span>
          </button>
        </div>

        <!-- 状态 B：搜索结果为空 -->
        <div
          v-else
          class="flex flex-col items-center justify-center h-[calc(100%-40px)] text-center py-20"
        >
          <div :class="['w-16 h-16 rounded-3xl border shadow-sm flex items-center justify-center text-slate-400 mb-4 backdrop-blur-md animate-float', isDarkMode ? 'bg-slate-800 border-slate-700' : 'bg-white border-slate-200/80']">
            <SearchX class="w-8 h-8" />
          </div>
          <h3 :class="['text-base font-bold mb-1', isDarkMode ? 'text-slate-200' : 'text-slate-900']">没有找到相关书签</h3>
          <p class="text-xs text-slate-500 mb-6 max-w-xs">没有找到与「<span :class="['font-semibold', isDarkMode ? 'text-slate-300' : 'text-slate-800']">{{ searchQuery }}</span>」相关的书签，换个关键词试试吧</p>
          <button
            @click="searchQuery = ''"
            :class="['px-5 py-2.5 rounded-xl text-xs font-semibold transition-all active:scale-95 cursor-pointer', isDarkMode ? 'bg-slate-800 hover:bg-slate-700 text-slate-300' : 'bg-slate-200/80 hover:bg-slate-200 text-slate-700']"
          >
            清除搜索条件
          </button>
        </div>
      </section>
    </main>

    <!-- 添加书签 Modal -->
    <Transition
      enter-active-class="transition duration-250 ease-out"
      enter-from-class="opacity-0"
      enter-to-class="opacity-100"
      leave-active-class="transition duration-200 ease-in"
      leave-from-class="opacity-100"
      leave-to-class="opacity-0"
    >
    <div
      v-if="isModalOpen"
      class="fixed inset-0 bg-slate-900/40 backdrop-blur-md z-50 flex items-center justify-center p-4"
    >
      <Transition
        appear
        enter-active-class="transition duration-250 ease-out"
        enter-from-class="opacity-0 scale-95"
        enter-to-class="opacity-100 scale-100"
        leave-active-class="transition duration-200 ease-in"
        leave-from-class="opacity-100 scale-100"
        leave-to-class="opacity-0 scale-95"
      >
      <div :class="['border rounded-3xl w-full max-w-md p-6 shadow-2xl backdrop-blur-2xl relative transition-colors duration-300', isDarkMode ? 'bg-slate-900/90 border-slate-800 text-slate-100' : 'bg-white/95 border-white text-slate-900']">
        <button
          @click="closeBookmarkModal"
          :class="['absolute top-5 right-5 p-1 rounded-full transition-colors cursor-pointer', isDarkMode ? 'text-slate-400 hover:text-slate-200 hover:bg-slate-800' : 'text-slate-400 hover:text-slate-600 hover:bg-slate-100']"
        >
          <X class="w-4 h-4" />
        </button>

        <h2 :class="['text-base font-bold mb-5', isDarkMode ? 'text-slate-100' : 'text-slate-900']">{{ isEditing ? '编辑书签' : '添加新书签' }}</h2>

        <form @submit.prevent="handleSubmitBookmark" class="space-y-4">
          <div>
            <label :class="['block text-sm font-semibold mb-1.5', isDarkMode ? 'text-slate-300' : 'text-slate-700']">网站链接 (URL)</label>
            <input
              v-model="newBookmark.url"
              type="text"
              required
              placeholder="输入链接后自动抓取标题和描述"
              @blur="handleUrlBlur"
              :class="['w-full px-4 py-2.5 border rounded-xl text-sm focus:outline-none focus:border-sky-400 transition-all shadow-xs', isDarkMode ? 'bg-slate-800 border-slate-700 text-slate-100' : 'bg-white border-slate-200 text-slate-900']"
            />
          </div>

          <div>
            <label :class="['flex items-center gap-2 text-sm font-semibold mb-1.5', isDarkMode ? 'text-slate-300' : 'text-slate-700']">
              网站名称
              <span v-if="isFetchingMeta" class="text-xs text-sky-500 font-normal animate-pulse">抓取中...</span>
            </label>
            <input
              v-model="newBookmark.title"
              type="text"
              :placeholder="isFetchingMeta ? '正在自动抓取...' : '留空则使用域名'"
              :class="['w-full px-4 py-2.5 border rounded-xl text-sm focus:outline-none focus:border-sky-400 transition-all shadow-xs', isDarkMode ? 'bg-slate-800 border-slate-700 text-slate-100' : 'bg-white border-slate-200 text-slate-900']"
            />
          </div>

          <div>
            <label :class="['block text-sm font-semibold mb-1.5', isDarkMode ? 'text-slate-300' : 'text-slate-700']">所属分类</label>
            <select
              v-model="newBookmark.category"
              :class="['w-full px-4 py-2.5 border rounded-xl text-sm focus:outline-none focus:border-sky-400 transition-all shadow-xs cursor-pointer', isDarkMode ? 'bg-slate-800 border-slate-700 text-slate-100' : 'bg-white border-slate-200 text-slate-900']"
            >
              <option v-for="cat in categories.filter(c => c.name !== '全部')" :key="cat.id" :value="cat.name">
                {{ cat.name }}
              </option>
            </select>
          </div>

          <div>
            <label :class="['flex items-center gap-2 text-sm font-semibold mb-1.5', isDarkMode ? 'text-slate-300' : 'text-slate-700']">
              描述 (可选)
              <span v-if="isFetchingMeta" class="text-xs text-sky-500 font-normal animate-pulse">抓取中...</span>
            </label>
            <textarea
              v-model="newBookmark.description"
              rows="3"
              @keydown.enter.exact.prevent="handleSubmitBookmark"
              :class="['w-full px-4 py-2.5 border rounded-xl text-sm focus:outline-none focus:border-sky-400 transition-all shadow-xs resize-none', isDarkMode ? 'bg-slate-800 border-slate-700 text-slate-100' : 'bg-white border-slate-200 text-slate-900']"
            ></textarea>
          </div>

          <div>
            <label :class="['flex items-center gap-2 text-sm font-semibold mb-1.5', isDarkMode ? 'text-slate-300' : 'text-slate-700']">
              图标 (可选)
              <span :class="['text-xs font-normal', isDarkMode ? 'text-slate-500' : 'text-slate-400']">自动获取失败时可手动设置</span>
            </label>
            <div class="flex items-center gap-2">
              <input
                v-model="newBookmark.customIcon"
                type="text"
                placeholder="粘贴图标 URL 或选择本地图片"
                :class="['flex-1 min-w-0 px-4 py-2.5 border rounded-xl text-sm focus:outline-none focus:border-sky-400 transition-all shadow-xs', isDarkMode ? 'bg-slate-800 border-slate-700 text-slate-100' : 'bg-white border-slate-200 text-slate-900']"
              />
              <button
                type="button"
                @click="iconFileRef?.click()"
                :class="['flex-shrink-0 p-2.5 border rounded-xl transition-all cursor-pointer', isDarkMode ? 'bg-slate-800 border-slate-700 text-slate-300 hover:text-sky-400 hover:border-sky-500' : 'bg-white border-slate-200 text-slate-500 hover:text-sky-500 hover:border-sky-400']"
                title="选择本地图片"
              >
                <ImagePlus class="w-4 h-4" />
              </button>
              <input ref="iconFileRef" type="file" accept="image/*" class="hidden" @change="handleIconFile" />
              <img
                v-if="newBookmark.customIcon && newBookmark.customIcon.startsWith('data:')"
                :src="newBookmark.customIcon"
                class="w-10 h-10 rounded-lg border object-contain flex-shrink-0"
                :class="isDarkMode ? 'border-slate-700 bg-slate-800' : 'border-slate-200 bg-white'"
              />
            </div>
          </div>

          <div class="flex gap-3 pt-2">
            <button
              type="button"
              @click="closeBookmarkModal"
              :class="['flex-1 py-3 rounded-xl text-sm font-semibold transition-colors cursor-pointer', isDarkMode ? 'bg-slate-800 hover:bg-slate-700 text-slate-300' : 'bg-slate-100 hover:bg-slate-200 text-slate-700']"
            >
              取消
            </button>
            <button
              type="submit"
              class="flex-1 py-3 bg-sky-500 hover:bg-sky-600 text-white rounded-xl text-sm font-semibold transition-colors shadow-md shadow-sky-500/20 cursor-pointer"
            >
              {{ isEditing ? '保存修改' : '保存' }}
            </button>
          </div>
        </form>
      </div>
      </Transition>
    </div>
    </Transition>

    <!-- 删除确认 Modal -->
    <Transition
      enter-active-class="transition duration-200 ease-out"
      enter-from-class="opacity-0"
      enter-to-class="opacity-100"
      leave-active-class="transition duration-150 ease-in"
      leave-from-class="opacity-100"
      leave-to-class="opacity-0"
    >
    <div
      v-if="confirmDialog"
      class="fixed inset-0 bg-slate-900/40 backdrop-blur-md z-[60] flex items-center justify-center p-4"
      @click.self="confirmDialog = null"
    >
      <Transition
        appear
        enter-active-class="transition duration-200 ease-out"
        enter-from-class="opacity-0 scale-95"
        enter-to-class="opacity-100 scale-100"
        leave-active-class="transition duration-150 ease-in"
        leave-from-class="opacity-100 scale-100"
        leave-to-class="opacity-0 scale-95"
      >
      <div :class="['border rounded-3xl w-full max-w-sm p-6 shadow-2xl backdrop-blur-2xl relative transition-colors duration-300', isDarkMode ? 'bg-slate-900/90 border-slate-800 text-slate-100' : 'bg-white/95 border-white text-slate-900']">
        <div class="flex items-center gap-3 mb-4">
          <div :class="['p-2.5 rounded-2xl', isDarkMode ? 'bg-red-950/40' : 'bg-red-50']">
            <AlertTriangle :class="['w-5 h-5', isDarkMode ? 'text-red-400' : 'text-red-500']" />
          </div>
          <h2 :class="['text-base font-bold', isDarkMode ? 'text-slate-100' : 'text-slate-900']">{{ confirmDialog.title }}</h2>
        </div>
        <p :class="['text-sm mb-6 pl-[52px]', isDarkMode ? 'text-slate-400' : 'text-slate-500']">{{ confirmDialog.message }}</p>
        <div class="flex gap-3">
          <button
            @click="confirmDialog = null"
            :class="['flex-1 py-3 rounded-xl text-sm font-semibold transition-colors cursor-pointer', isDarkMode ? 'bg-slate-800 hover:bg-slate-700 text-slate-300' : 'bg-slate-100 hover:bg-slate-200 text-slate-700']"
          >
            取消
          </button>
          <button
            @click="handleConfirm"
            class="flex-1 py-3 bg-red-500 hover:bg-red-600 text-white rounded-xl text-sm font-semibold transition-colors shadow-md shadow-red-500/20 cursor-pointer"
          >
            确认删除
          </button>
        </div>
      </div>
      </Transition>
    </div>
    </Transition>

    <!-- Toast 提示 -->
    <Transition
      enter-active-class="transition duration-300 ease-out"
      enter-from-class="opacity-0 -translate-y-3"
      enter-to-class="opacity-100 translate-y-0"
      leave-active-class="transition duration-200 ease-in"
      leave-from-class="opacity-100 translate-y-0"
      leave-to-class="opacity-0 -translate-y-3"
    >
      <div
        v-if="toastMessage"
        :class="['fixed top-6 left-1/2 -translate-x-1/2 z-[70] px-5 py-2.5 rounded-2xl text-sm font-medium shadow-lg backdrop-blur-xl border', isDarkMode ? 'bg-slate-800/90 text-slate-200 border-slate-700 shadow-black/30' : 'bg-white/95 text-slate-700 border-slate-200/80 shadow-slate-200/50']"
      >
        {{ toastMessage }}
      </div>
    </Transition>

    <!-- 添加分类 Modal -->
    <Transition
      enter-active-class="transition duration-250 ease-out"
      enter-from-class="opacity-0"
      enter-to-class="opacity-100"
      leave-active-class="transition duration-200 ease-in"
      leave-from-class="opacity-100"
      leave-to-class="opacity-0"
    >
    <div
      v-if="isCategoryModalOpen"
      class="fixed inset-0 bg-slate-900/40 backdrop-blur-md z-50 flex items-center justify-center p-4"
    >
      <Transition
        appear
        enter-active-class="transition duration-250 ease-out"
        enter-from-class="opacity-0 scale-95"
        enter-to-class="opacity-100 scale-100"
        leave-active-class="transition duration-200 ease-in"
        leave-from-class="opacity-100 scale-100"
        leave-to-class="opacity-0 scale-95"
      >
      <div :class="['border rounded-3xl w-full max-w-sm p-6 shadow-2xl backdrop-blur-2xl relative transition-colors duration-300', isDarkMode ? 'bg-slate-900/90 border-slate-800 text-slate-100' : 'bg-white/95 border-white text-slate-900']">
        <button
          @click="isCategoryModalOpen = false"
          :class="['absolute top-5 right-5 p-1 rounded-full transition-colors cursor-pointer', isDarkMode ? 'text-slate-400 hover:text-slate-200 hover:bg-slate-800' : 'text-slate-400 hover:text-slate-600 hover:bg-slate-100']"
        >
          <X class="w-4 h-4" />
        </button>

        <h2 :class="['text-base font-bold mb-5', isDarkMode ? 'text-slate-100' : 'text-slate-900']">添加新分类</h2>

        <form @submit.prevent="handleAddCategory" class="space-y-4">
          <div>
            <label :class="['block text-sm font-semibold mb-1.5', isDarkMode ? 'text-slate-300' : 'text-slate-700']">分类名称</label>
            <input
              v-model="newCategoryName"
              type="text"
              required
              :class="['w-full px-4 py-2.5 border rounded-xl text-sm focus:outline-none focus:border-sky-400 transition-all shadow-xs', isDarkMode ? 'bg-slate-800 border-slate-700 text-slate-100' : 'bg-white border-slate-200 text-slate-900']"
            />
          </div>

          <div class="flex gap-3 pt-2">
            <button
              type="button"
              @click="isCategoryModalOpen = false"
              :class="['flex-1 py-3 rounded-xl text-sm font-semibold transition-colors cursor-pointer', isDarkMode ? 'bg-slate-800 hover:bg-slate-700 text-slate-300' : 'bg-slate-100 hover:bg-slate-200 text-slate-700']"
            >
              取消
            </button>
            <button
              type="submit"
              class="flex-1 py-3 bg-sky-500 hover:bg-sky-600 text-white rounded-xl text-sm font-semibold transition-colors shadow-md shadow-sky-500/20 cursor-pointer"
            >
              确认添加
            </button>
          </div>
        </form>
      </div>
      </Transition>
    </div>
    </Transition>

  </div>
</template>

<style scoped>
.custom-app-container {
  font-family: "LXGW WenKai GB Screen", "LXGW WenKai", system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
}

@keyframes pulseSlow {
  0%, 100% { transform: scale(1) translate(0, 0); opacity: 0.5; }
  50% { transform: scale(1.06) translate(10px, -10px); opacity: 0.7; }
}
@keyframes pulseSlower {
  0%, 100% { transform: scale(1) translate(0, 0); opacity: 0.5; }
  50% { transform: scale(1.08) translate(-15px, 10px); opacity: 0.65; }
}

.animate-pulse-slow {
  animation: pulseSlow 14s ease-in-out infinite;
}
.animate-pulse-slower {
  animation: pulseSlower 18s ease-in-out infinite;
}

.card-list-move {
  transition: all 0.3s cubic-bezier(0.4, 0, 0.2, 1);
}

.card-list-enter-active {
  animation: cardEnter 0.35s cubic-bezier(0.16, 1, 0.3, 1) both;
  animation-delay: var(--stagger, 0ms);
}

.card-list-leave-active {
  transition: all 0.25s cubic-bezier(0.4, 0, 0.2, 1);
  position: absolute;
}

.card-list-leave-to {
  opacity: 0;
  transform: scale(0.95) translateY(8px);
}

@keyframes cardEnter {
  from {
    opacity: 0;
    transform: scale(0.92) translateY(12px);
  }
  to {
    opacity: 1;
    transform: scale(1) translateY(0);
  }
}

.custom-scrollbar::-webkit-scrollbar {
  width: 5px;
}
.custom-scrollbar::-webkit-scrollbar-track {
  background: transparent;
}
.custom-scrollbar::-webkit-scrollbar-thumb {
  background: rgba(203, 213, 225, 0.4);
  border-radius: 6px;
}
.custom-scrollbar::-webkit-scrollbar-thumb:hover {
  background: rgba(148, 163, 184, 0.7);
}

.dark-scrollbar::-webkit-scrollbar-thumb {
  background: rgba(100, 116, 139, 0.25);
}
.dark-scrollbar::-webkit-scrollbar-thumb:hover {
  background: rgba(100, 116, 139, 0.5);
}

.cat-drop-target {
  box-shadow: inset 0 2px 0 0 rgb(56, 189, 248);
  border-radius: 1rem;
}

@keyframes float {
  0%, 100% { transform: translateY(0); }
  50% { transform: translateY(-8px); }
}
.animate-float {
  animation: float 3s ease-in-out infinite;
}

:deep(.search-highlight) {
  background: rgba(56, 189, 248, 0.25);
  color: inherit;
  border-radius: 2px;
  padding: 0 2px;
}
</style>
