import { invoke } from "@tauri-apps/api/core";

// 定义你期望的数据结构
export interface BookmarkData {
  categories: { id: number; name: string }[];
  bookmarks: any[];
}

// 读取书签数据
export async function loadBookmarks(): Promise<BookmarkData> {
  try {
    // 调用后端刚才写的 load_bookmarks 函数
    const jsonStr = await invoke<string>('load_bookmarks');
    // 把 JSON 字符串转回对象
    return JSON.parse(jsonStr) as BookmarkData;
  } catch (error) {
    console.error('读取书签数据失败:', error);
    // 读取失败时返回一个空的数据结构，防止前端崩溃
    return { categories: [], bookmarks: [] };
  }
}
// 保存书签数据
export async function saveBookmarks(data: BookmarkData): Promise<void> {
  try {
    // 调用后端刚才写的 save_bookmarks 函数，传入 JSON 字符串
    await invoke('save_bookmarks', { content: JSON.stringify(data, null, 2) });
    console.log('✅ 书签数据已成功保存到本地文件!');
  } catch (error) {
    console.error('保存书签数据失败:', error);
    // 增加一个弹窗，帮我们直接看到报错原因！
    alert('❌ 保存失败！请按 F12 查看控制台具体报错信息:\n' + error);
  }
}

// 导出书签：弹出保存对话框，返回 true 表示用户实际完成了保存，false 表示取消了对话框
export async function exportBookmarks(): Promise<boolean> {
  return await invoke<boolean>('export_bookmarks');
}

// 导入书签：弹出打开对话框并校验格式，取消对话框时返回 null；
// 校验失败会抛出错误（含具体原因），交由调用方展示给用户
export async function importBookmarks(): Promise<BookmarkData | null> {
  const content = await invoke<string | null>('import_bookmarks');
  if (!content) return null;
  return JSON.parse(content) as BookmarkData;
}