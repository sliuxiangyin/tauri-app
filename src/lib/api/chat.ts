import { ChatMessage } from '@/stores/useChatStore'
import { invoke } from '@tauri-apps/api/core'

export async function getMessages(accountId: string, sessionId: string) {
    return await invoke<ChatMessage[]>('get_messages', {
        accountId: accountId,
        sessionId: sessionId,
    })
}

export async function clearMessages(accountId: string, sessionId?: string) {
    return await invoke<number>('clear_messages', {
        accountId: accountId,
        sessionId: sessionId ?? null,
    })
}

export async function getSessions(accountId: string) {
    return await invoke<Array<{
        session_id: string
        name: string
        message_count: number
        last_message_at?: string
    }>>('get_sessions', {
        accountId: accountId,
    })
}
