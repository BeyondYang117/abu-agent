import type { ChatUserProfile } from './userProfile'

const APP_ICON_SRC = '/icon.png'

type UserAvatarProps = {
  profile: ChatUserProfile
  size?: number
  className?: string
}

function AppLogoAvatar({ size, className }: { size: number; className?: string }) {
  return (
    <div
      className={`flex shrink-0 items-center justify-center overflow-hidden rounded-full bg-white ring-1 ring-black/[0.05] dark:bg-neutral-900 dark:ring-white/[0.08] ${className ?? ''}`}
      style={{ width: size, height: size }}
      aria-hidden
    >
      <img
        src={APP_ICON_SRC}
        alt=""
        className="h-[82%] w-[82%] object-contain"
        draggable={false}
      />
    </div>
  )
}

export function UserAvatar({ profile, size = 28, className }: UserAvatarProps) {
  if (profile.avatarUrl) {
    return (
      <img
        src={profile.avatarUrl}
        alt=""
        width={size}
        height={size}
        className={`shrink-0 rounded-full object-cover ring-1 ring-black/[0.05] dark:ring-white/[0.08] ${className ?? ''}`}
      />
    )
  }

  return <AppLogoAvatar size={size} className={className} />
}
