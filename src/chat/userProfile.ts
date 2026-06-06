export type ChatUserProfile = {
  displayName: string
  avatarUrl: string
}

export function resolveChatUserProfile(
  chat?: { userDisplayName?: string; userAvatar?: string } | null,
): ChatUserProfile {
  return {
    displayName: chat?.userDisplayName?.trim() || '',
    avatarUrl: chat?.userAvatar?.trim() || '',
  }
}

export function hasChatDisplayName(profile: ChatUserProfile): boolean {
  return profile.displayName.length > 0
}
