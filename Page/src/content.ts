export type Locale = 'en' | 'zh'
export type DemoMode = 'media' | 'notification' | 'widgets'
export type DocKey =
  | 'guide'
  | 'getting-started'
  | 'download'
  | 'plugin-dev'
  | 'plugin-dev/quickstart'
  | 'plugin-dev/abi-lifecycle'
  | 'plugin-dev/services'
  | 'plugin-dev/api'
  | 'plugin-dev/api/context'
  | 'plugin-dev/api/activity'
  | 'plugin-dev/api/system'
  | 'plugin-dev/api/drawing'
  | 'plugin-dev/api/media'
  | 'plugin-dev/api/i18n'
  | 'plugin-dev/api/host-state'
  | 'plugin-dev/api/widget'
  | 'plugin-dev/api/surface'
  | 'plugin-dev/api/input'
  | 'plugin-dev/api/events'
  | 'plugin-dev/api/command'
  | 'plugin-dev/api/media-session'
  | 'plugin-dev/api/lyrics-transform'
  | 'plugin-dev/api/settings'
  | 'plugin-dev/api/text'
  | 'plugin-dev/api/image'
  | 'plugin-dev/api/store'
  | 'plugin-dev/api/log'
  | 'plugin-dev/packaging'
  | 'api-changelog'
  | 'changelog'

export const DOC_KEYS: DocKey[] = [
  'guide',
  'getting-started',
  'download',
  'plugin-dev',
  'plugin-dev/quickstart',
  'plugin-dev/abi-lifecycle',
  'plugin-dev/services',
  'plugin-dev/api',
  'plugin-dev/api/context',
  'plugin-dev/api/activity',
  'plugin-dev/api/system',
  'plugin-dev/api/drawing',
  'plugin-dev/api/media',
  'plugin-dev/api/i18n',
  'plugin-dev/api/host-state',
  'plugin-dev/api/widget',
  'plugin-dev/api/surface',
  'plugin-dev/api/input',
  'plugin-dev/api/events',
  'plugin-dev/api/command',
  'plugin-dev/api/media-session',
  'plugin-dev/api/lyrics-transform',
  'plugin-dev/api/settings',
  'plugin-dev/api/text',
  'plugin-dev/api/image',
  'plugin-dev/api/store',
  'plugin-dev/api/log',
  'plugin-dev/packaging',
  'api-changelog',
  'changelog',
]

export const localePath = (locale: Locale, path = '/') => {
  const cleanPath = path.startsWith('/') ? path : `/${path}`
  if (locale === 'zh') return cleanPath === '/' ? '/zh' : `/zh${cleanPath}`
  return cleanPath
}

export const copy = {
  en: {
    language: '中文',
    nav: {
      product: 'WinIsland',
      winwidget: 'WinWidget',
      liquid: 'Technology',
      guide: 'Guide',
      changelog: 'Changelog',
      download: 'Download',
    },
    liquid: {
      eyebrow: 'WinIsland Technology',
      tagline: 'Real optics, on the Windows desktop',
      body: 'Windows has no system material for this. WinIsland captures the desktop behind it and computes refraction, dispersion, and highlights itself, for every pixel of the glass.',
      tryIt: 'Try the glass',
      source: 'Read the source',
      demo: {
        title: 'Move the sliders. Bend the light.',
        body: 'A small web demo of the three controls that shape the look.',
        refraction: 'Refraction',
        dispersion: 'Dispersion',
        blur: 'Blur',
        note: 'This demo approximates the effect with web filters. The app itself uses the Skia shader described below.',
      },
      pipeline: {
        title: 'From desktop to glass',
        body: 'Each frame passes through four stages.',
        steps: [
          { title: 'Capture', body: 'A DWM shared desktop visual and Windows Graphics Capture deliver the desktop as a GPU texture.' },
          { title: 'Shape', body: 'A signed distance field tells each pixel how far it is from the edge.' },
          { title: 'Refract', body: 'A curved bezel bends rays, and each color channel samples its own offset.' },
          { title: 'Finish', body: 'Tint, rim highlights, and edge anti-aliasing complete the surface.' },
        ],
      },
      chapters: [
        {
          title: 'See behind the island, without seeing itself',
          body: 'Glass needs the pixels behind it, but capturing the screen would also capture the island and feed it back forever. WinIsland asks DWM for a live visual of the shared desktop with its own windows left out, composites that over the wallpaper in an offscreen host, and captures the visual through Windows Graphics Capture on a worker thread. The island’s display affinity never changes, so it still appears in your screenshots and recordings. Windows 10 and 11 builds differ in this interface, and the code selects the matching one at runtime.',
        },
        {
          title: 'Corners that flow, shapes that merge',
          body: 'The outline is a signed distance field. Corners use a superellipse with exponent 2.5 instead of a circular arc, so curvature eases in continuously. When the island splits or joins, two shapes combine through a smooth union, and both distance and gradient stay continuous.',
        },
        {
          title: 'A bezel with real curvature',
          body: 'Light only bends at the rim. Inside the bezel, the distance to the edge becomes a surface slope that steepens toward the boundary, and the slope plus the outline gradient form a 3D normal. Past the bezel the slope is zero, so the center stays perfectly clear.',
        },
        {
          title: 'Snell’s law, per pixel',
          body: 'A straight-down view ray is refracted through that normal with an index of refraction of 1.45, close to real glass. Traveling through the glass thickness turns the bent ray into a 2D offset, and the shader samples the desktop there.',
        },
        {
          title: 'A hint of color at the edge',
          body: 'Real glass bends each wavelength differently. The shader samples red, green, and blue at slightly different offsets, and the spread briefly widens while the island changes shape.',
        },
        {
          title: 'Bright enough to feel lit, calm enough to read',
          body: 'The rim catches a directional light from above, with a softer reflection on the opposite side. Measured luminance drives the tint, so the glass darkens over bright wallpapers and text stays legible.',
        },
        {
          title: 'Quality only where it shows',
          body: 'Only pixels inside the bezel compute a normal and refract. When the resulting offset is too small to see, a single sample replaces the three-channel lookup, so the clear center of the glass stays cheap.',
        },
      ],
      result: {
        title: 'The numbers behind the material',
        body: 'Default values for the island surface, straight from the renderer.',
        imageAlt: 'The WinIsland Dynamic Island on a Windows desktop',
        stats: [
          { value: '1.45', label: 'Index of refraction' },
          { value: '18 px', label: 'Bezel width at 100% scale' },
          { value: '14 px', label: 'Glass thickness at 100% scale' },
          { value: '0.08', label: 'Default dispersion' },
        ],
      },
      closing: {
        title: 'See it on your own wallpaper',
        body: 'Liquid glass is one of the island styles in WinIsland.',
        action: 'Download WinIsland',
      },
    },
    winwidget: {
      tagline: 'The next generation of desktop widgets',
      body: 'Glass widgets that live on your wallpaper, showing the weather, your music, and the moon, always just above the desktop.',
      source: 'View on GitHub',
      explore: 'See the widgets',
      soon: 'Coming soon',
      imageAlt: 'WinWidget Desktop weather, music, moon, clock, calendar and battery widgets on a glowing dot wallpaper',
      widgets: {
        title: 'A widget for every glance',
        body: 'Pick from the gallery, drag onto the desktop, and choose the size that fits.',
        items: [
          { title: 'Weather', sizes: 'Small · Medium', body: 'Current conditions and the next few hours for where you are, located down to your district and refreshed every half hour.' },
          { title: 'Music', sizes: 'Capsule · Small · Medium · Large', body: 'Artwork, progress, and playback controls for whatever is playing. Tap the output icon to change devices and volume.' },
          { title: 'Moon', sizes: 'Small · Medium · Large', body: 'A NASA photo of the moon, shaded for today’s phase, with moonrise, moonset, and illumination. The large size adds a two-week calendar.' },
          { title: 'Clock', sizes: 'Small', body: 'Four faces, Classic, Digital, Minimal, and Square, to keep the time in view.' },
          { title: 'Calendar', sizes: 'Small · Medium · Large', body: 'The day and date at a glance, with the lunar date beside them. Larger sizes show the whole month.' },
          { title: 'Battery', sizes: 'Small · Medium · Large', body: 'Power status for this PC and any Bluetooth device that reports its level, from a single ring to a full list.' },
        ],
      },
      flow: {
        title: 'On your desktop in four steps',
        body: 'No configuration files to learn. Everything happens right where the widgets live.',
        steps: [
          { title: 'Open the gallery', body: 'Choose Edit Widgets from the tray or from any widget’s menu. The gallery rises from just above the taskbar, with search and previews in your current glass style.' },
          { title: 'Drag it out', body: 'Drag a widget onto the desktop and drop it where you want it, or click to add it right away.' },
          { title: 'Let it snap', body: 'Widgets land on a square grid. The grid scale and gap settings size everything at once, Tidy lines them back up, and Lock Layout keeps them in place.' },
          { title: 'Find the right size', body: 'While the gallery is open, drag a widget’s corner to resize it or use the button at its top left to remove it. A widget’s menu also changes its size and clock face.' },
        ],
      },
      glass: {
        title: 'Two kinds of glass',
        body: 'Pick the one that suits your wallpaper. Each widget uses the style you set.',
        items: [
          { label: 'Liquid glass', title: 'Your wallpaper, bent at the edges', body: 'Refraction, dispersion, blur, and tint are all separate settings. Sample only the wallpaper, or let the glass pick up the windows behind it.', points: ['Refraction', 'Dispersion', 'Blur', 'Tint'] },
          { label: 'Frosted glass', title: 'A soft wash of color', body: 'The desktop melts into a gentle haze, so the content stays calm. Set the blur and tint to taste.', points: ['Blur', 'Tint'] },
        ],
      },
      traits: {
        title: 'Part of the desktop',
        items: [
          { label: 'Glass', title: 'Liquid or frosted', body: 'Choose a glass style that bends and blurs your live wallpaper.' },
          { label: 'Layout', title: 'A tidy square grid', body: 'Widgets snap to square cells, and one scale setting sizes them all.' },
          { label: 'Layer', title: 'Always underneath', body: 'Widgets stay pinned below every window, so they never cover your work.' },
        ],
      },
      performance: {
        title: 'Quiet when you are busy',
        body: 'Built from the same native foundation as WinIsland. Widgets only work when there is something to show, so they stay out of your apps’ way.',
        items: [
          { label: 'Core', value: 'Rust', body: 'Memory-safe and lightweight' },
          { label: 'Rendering', value: 'Skia', body: 'GPU-drawn glass and motion' },
          { label: 'Efficiency', value: 'Pauses when covered', body: 'Hidden widgets stop drawing' },
        ],
        details: [
          { title: 'One capture per display', body: 'Every widget on a display shares a single desktop capture.' },
          { title: 'Draws on change', body: 'Unchanged content is reused, and liquid glass redraws at a capped rate.' },
          { title: 'Stops with the screen', body: 'When every widget on a display is covered, its capture stops too.' },
          { title: 'Gives memory back', body: 'GPU resources that sit idle are released on their own.' },
        ],
      },
      closing: {
        title: 'WinWidget Desktop is on its way',
        body: 'Follow the repository to see it when it lands.',
      },
    },
    hero: {
      title: 'A small island\nA better desktop',
      body: 'Media, notifications, widgets, and the controls you need — always close, never in the way.',
      primary: 'Download for Windows',
      secondary: 'See how it works',
      eyebrow: 'Live activities for Windows',
    },
    capabilities: ['Live lyrics', 'Audio spectrum', 'Media controls', 'Windows notifications', 'Volume and brightness', 'Resource usage', 'Privacy indicator', 'Plugin marketplace', 'Liquid glass', 'Hides in full screen'],
    showcase: {
      title: 'A closer look at the island',
      body: 'One small surface at the top of your screen, with a lot going on inside it.',
      items: [
        {
          label: 'Music',
          title: 'Artwork, lyrics, and sound together',
          body: 'WinIsland reads what is playing through the Windows media controls, then shows the artwork, track, and progress with playback controls close by. Audio is analyzed in memory only, just enough to drive the spectrum.',
          points: ['AMLL, LRCLIB, NetEase, QQ, and Kugou lyrics', 'Local LRC folders', 'Choose which apps it follows'],
        },
        {
          label: 'Alerts',
          title: 'Messages at a glance',
          body: 'Windows notifications can appear on the island with the app icon, title, and text in one quiet line. The feature is off by default, so it is your call.',
          points: ['App icon and title', 'Off by default', 'Stays on your PC'],
        },
        {
          label: 'Widgets',
          title: 'What you check most, one click away',
          body: 'Put a clock, a calendar, and resource usage on the expanded view, and keep an eye on CPU, memory, GPU, network, and disk. Plugins can add widgets of their own.',
          points: ['Clock and calendar', 'Resource usage', 'Plugin widgets'],
        },
        {
          label: 'Appearance',
          title: 'A different look, still your desktop',
          body: 'Switch between island styles, including liquid glass, and use a font of your own. When a game or video goes full screen, the island can step out of the way.',
          points: ['Several island styles', 'Custom fonts', 'Hides in full screen'],
        },
        {
          label: 'Details',
          title: 'The small things Windows leaves out',
          body: 'The volume keys get a cleaner overlay. Copy a link and the island offers to open it. When the microphone or camera is in use, you can see it. There are no accounts, ads, or telemetry.',
          points: ['Volume and brightness', 'Copied links', 'Privacy indicator'],
        },
      ],
    },
    glass: {
      title: 'Glass you can calculate',
      body: 'Windows has no ready-made liquid glass. WinIsland captures the desktop behind the island and uses a GPU shader to work out refraction, dispersion, and highlights for every pixel, so the island settles into your wallpaper.',
      points: ['Snell refraction at IOR 1.45', 'Continuous superellipse corners', 'Dispersion across red, green, and blue'],
      action: 'See how it works',
      imageAlt: 'The WinIsland island on a Windows desktop',
    },
    plugins: {
      title: 'Plugins take it further',
      body: 'Plugins are native DLLs loaded through a stable ABI. Install them from the marketplace, or write your own in Rust.',
      items: [
        { title: 'Status text', body: 'Post a line of text on the island, then update or remove it when things change.' },
        { title: 'Media sources', body: 'Publish track information and handle the playback buttons for your own source.' },
        { title: 'Custom widgets', body: 'Draw your own content into the expanded widget grid.' },
        { title: 'Settings pages', body: 'Describe controls for your plugin and store what people choose.' },
        { title: 'Commands and shortcuts', body: 'Register commands, with tray entries and keyboard shortcuts when you need them.' },
        { title: 'Lyric transforms', body: 'Process parsed lyric lines before they reach the screen.' },
      ],
    },
    moments: {
      title: 'It changes with what matters now',
      body: 'WinIsland stays compact until there is something worth seeing. Click once to reveal the details and controls you need.',
      modes: {
        media: 'Media',
        notification: 'Alerts',
        widgets: 'Widgets',
      },
    },
    features: {
      title: 'Made to feel like it belongs',
      items: [
        {
          title: 'Your music, in motion',
          body: 'Album art, playback controls, live lyrics, and an audio spectrum come together in one fluid view.',
          accent: 'coral',
        },
        {
          title: 'Spring, not snap',
          body: 'Physics-based transitions make every expansion and return feel responsive without calling attention to themselves.',
          accent: 'blue',
        },
        {
          title: 'Yours to arrange',
          body: 'Choose the widgets and appearance that fit your desktop. WinIsland adapts instead of taking over.',
          accent: 'green',
        },
      ],
    },
    performance: {
      title: 'Native at every layer',
      body: 'A compact Windows experience, built from a small set of fast, dependable parts.',
      items: [
        { label: 'Core', value: 'Rust', body: 'Memory-safe and lightweight' },
        { label: 'Rendering', value: 'Skia', body: 'Hardware-accelerated motion' },
        { label: 'Platform', value: 'Windows', body: 'Native media and system APIs' },
      ],
    },
    open: {
      title: 'Make the island yours',
      body: 'Explore the source, help shape the plugin system, or build something the community has not imagined yet.',
      source: 'View on GitHub',
      plugins: 'Plugin guide',
    },
    download: {
      title: 'Bring WinIsland to your desktop',
      body: 'For Windows 10 version 2004 or later and Windows 11. 64-bit only.',
      stable: 'Download latest release',
      nightly: 'Get nightly build',
      mirror: 'Download from mirror',
      nightlyHint: 'Nightly builds include the newest changes and may be less stable.',
    },
    footer: {
      product: 'Product',
      resources: 'Resources',
      community: 'Community',
      license: 'Released under the GNU GPL v3 License.',
    },
    docs: {
      title: 'WinIsland Guide',
      subtitle: 'Everything you need to install, use, and extend WinIsland.',
      onThisPage: 'Documentation',
      loadError: 'This document could not be loaded. Refresh the page to try again.',
      pages: {
        guide: 'What is WinIsland?',
        'getting-started': 'Getting started',
        download: 'Download',
        'plugin-dev': 'Plugin development',
        'plugin-dev/quickstart': 'Plugin quickstart',
        'plugin-dev/abi-lifecycle': 'ABI and lifecycle',
        'plugin-dev/services': 'Host services',
        'plugin-dev/api': 'API reference',
        'plugin-dev/api/context': 'Context API',
        'plugin-dev/api/activity': 'Activity API',
        'plugin-dev/api/system': 'System API',
        'plugin-dev/api/drawing': 'Drawing SDK',
        'plugin-dev/api/media': 'Media API',
        'plugin-dev/api/i18n': 'I18n API',
        'plugin-dev/api/host-state': 'Host State API',
        'plugin-dev/api/widget': 'Widget API',
        'plugin-dev/api/surface': 'Surface API',
        'plugin-dev/api/input': 'Input API',
        'plugin-dev/api/events': 'Events API',
        'plugin-dev/api/command': 'Command API',
        'plugin-dev/api/media-session': 'Media Session API',
        'plugin-dev/api/lyrics-transform': 'Lyrics Transform API',
        'plugin-dev/api/settings': 'Settings API',
        'plugin-dev/api/text': 'Text API',
        'plugin-dev/api/image': 'Image API',
        'plugin-dev/api/store': 'Store API',
        'plugin-dev/api/log': 'Log API',
        'plugin-dev/packaging': 'Packaging and installation',
        'api-changelog': 'API changelog',
        changelog: 'Changelog',
      },
    },
    notFound: {
      title: 'This island is empty',
      body: 'The page you are looking for has moved or does not exist.',
      action: 'Return home',
    },
  },
  zh: {
    language: 'EN',
    nav: {
      product: 'WinIsland',
      winwidget: 'WinWidget',
      liquid: '技术',
      guide: '指南',
      changelog: '更新日志',
      download: '下载',
    },
    liquid: {
      eyebrow: 'WinIsland 技术解析',
      tagline: '在 Windows 桌面上，计算真实的光学',
      body: 'Windows 没有现成的这种材质。WinIsland 捕获身后的桌面，自己为玻璃的每一个像素计算折射、色散和高光。',
      tryIt: '亲手试试',
      source: '阅读源码',
      demo: {
        title: '拖动滑块，弯折光线',
        body: '一个网页小演示，感受决定玻璃观感的三个参数。',
        refraction: '折射',
        dispersion: '色散',
        blur: '模糊',
        note: '这里用网页滤镜近似效果，应用本身使用下文介绍的 Skia 着色器。',
      },
      pipeline: {
        title: '从桌面到玻璃',
        body: '每一帧都会经过四个阶段。',
        steps: [
          { title: '捕获', body: 'DWM 共享桌面视觉配合 Windows Graphics Capture，把桌面作为 GPU 纹理送来。' },
          { title: '塑形', body: '有向距离场告诉每个像素，它离边缘还有多远。' },
          { title: '折射', body: '弧形边缘弯折光线，每个颜色通道各自采样不同的偏移。' },
          { title: '收尾', body: '染色、边缘高光和抗锯齿，完成最终的表面。' },
        ],
      },
      chapters: [
        {
          title: '看见身后，却不看见自己',
          body: '玻璃需要身后的画面，但直接捕获屏幕会把灵动岛自己也拍进去，形成无限反馈。WinIsland 向 DWM 请求一份排除了自身窗口的共享桌面实时视觉，在离屏窗口里叠到壁纸之上，再由工作线程通过 Windows Graphics Capture 捕获。窗口的显示属性全程不变，所以截图和录屏里依然能看到灵动岛。Windows 10 与 11 的这组接口有差异，代码会在运行时选择匹配的版本。',
        },
        {
          title: '流动的圆角，融合的形状',
          body: '轮廓由有向距离场描述。圆角使用指数为 2.5 的超椭圆而不是圆弧，曲率因此连续过渡。灵动岛分裂或合并时，两个形状通过平滑并集结合，距离和梯度都保持连续。',
        },
        {
          title: '真正有弧度的边缘',
          body: '光线只在边缘发生弯折。在边框范围内，到边缘的距离被换算成越靠外越陡的表面斜率，再和轮廓梯度组成三维法线。超出边框后斜率为零，所以中央依然通透。',
        },
        {
          title: '逐像素的斯涅尔定律',
          body: '垂直向下的视线穿过这条法线发生折射，折射率为 1.45，接近真实玻璃。折射后的光线穿过玻璃厚度，得到一个二维偏移，着色器就在那里采样桌面。',
        },
        {
          title: '边缘的一抹彩色',
          body: '真实玻璃对不同波长的弯折程度不同。着色器在略有差异的偏移上分别采样红、绿、蓝，灵动岛变形时色散还会短暂增强。',
        },
        {
          title: '有光泽，也看得清',
          body: '边缘接收来自正上方的定向光，对侧有较柔和的反光。染色强度随背景亮度变化，壁纸越亮玻璃越深，文字始终清晰。',
        },
        {
          title: '只在看得出的地方花力气',
          body: '只有边框范围内的像素才计算法线和折射。偏移小到看不出来时，用一次采样代替三通道采样，玻璃通透的中央部分因此几乎没有额外开销。',
        },
      ],
      result: {
        title: '材质背后的数字',
        body: '灵动岛表面的默认参数，直接来自渲染代码。',
        imageAlt: 'Windows 桌面上的 WinIsland 灵动岛',
        stats: [
          { value: '1.45', label: '折射率' },
          { value: '18 px', label: '边框宽度（100% 缩放）' },
          { value: '14 px', label: '玻璃厚度（100% 缩放）' },
          { value: '0.08', label: '默认色散' },
        ],
      },
      closing: {
        title: '放到你自己的壁纸上看看',
        body: '液态玻璃是 WinIsland 的灵动岛风格之一。',
        action: '下载 WinIsland',
      },
    },
    winwidget: {
      tagline: '下一代桌面小组件',
      body: '玻璃质感的小组件，安静地待在壁纸之上。天气、音乐、月相，抬眼就能看到。',
      source: '在 GitHub 查看',
      explore: '看看小组件',
      soon: '即将推出',
      imageAlt: 'WinWidget Desktop 的天气、音乐、月相、时钟、日历和电量小组件，背景是发光点阵壁纸',
      widgets: {
        title: '抬眼一瞥，尽在桌面',
        body: '从小组件库里挑选，拖到桌面，再选一个合适的尺寸。',
        items: [
          { title: '天气', sizes: '小 · 中', body: '当前天气与未来几小时预报，自动定位到你所在的区县，每半小时刷新一次。' },
          { title: '音乐', sizes: '胶囊 · 小 · 中 · 大', body: '专辑封面、播放进度与控制，正在播放的内容都在这里。点按输出图标，还能切换播放设备、调节音量。' },
          { title: '月相', sizes: '小 · 中 · 大', body: '用 NASA 月面照片按当天月相着色，附月出月落和照射范围。大尺寸还有未来两周的月相日历。' },
          { title: '时钟', sizes: '小', body: '经典、数字、简约、方形四种表盘，时间始终在视线之内。' },
          { title: '日历', sizes: '小 · 中 · 大', body: '星期与日期一目了然，农历也在旁边。尺寸大一些，就能看到整个月。' },
          { title: '电量', sizes: '小 · 中 · 大', body: '这台电脑的电源状态，以及能上报电量的蓝牙设备。从单个圆环到完整列表，按尺寸呈现。' },
        ],
      },
      flow: {
        title: '四步，放上桌面',
        body: '不用研究配置文件，所有操作都在小组件所在的地方完成。',
        steps: [
          { title: '打开小组件库', body: '在托盘或小组件的菜单里选择“编辑小组件”，面板从任务栏上方升起。可以搜索，预览也会使用你当前的玻璃风格。' },
          { title: '拖到桌面', body: '把喜欢的小组件拖到桌面，放在你想要的位置，或者点按直接添加。' },
          { title: '自动对齐', body: '小组件会吸附到方形网格上。比例尺和间距统一调整大小，“整理”让它们重新排齐，“锁定布局”防止误碰。' },
          { title: '调到合适的尺寸', body: '编辑状态下，拖动小组件的右下角改变尺寸，点左上角的按钮移除。小组件的菜单里也能切换尺寸和表盘。' },
        ],
      },
      glass: {
        title: '两种玻璃，随你选',
        body: '挑一种和你的壁纸相配的风格，所有小组件都会跟着变。',
        items: [
          { label: '液态玻璃', title: '让壁纸在边缘弯折', body: '折射、色散、模糊和染色都能分别调节。可以只取壁纸，也可以让玻璃映出身后的窗口。', points: ['折射', '色散', '模糊', '染色'] },
          { label: '毛玻璃', title: '一层柔和的光', body: '桌面化作一片朦胧的色彩，内容显得格外安静。模糊和染色都可以按喜好调整。', points: ['模糊', '染色'] },
        ],
      },
      traits: {
        title: '生来就属于桌面',
        items: [
          { label: '玻璃', title: '液态或磨砂', body: '两种玻璃风格可选，折射并模糊你的动态壁纸。' },
          { label: '布局', title: '整齐的方形网格', body: '小组件自动吸附到方形格子，一个比例设置统一调整大小。' },
          { label: '层级', title: '永远在窗口之下', body: '始终固定在所有窗口下方，绝不遮挡你的工作。' },
        ],
      },
      performance: {
        title: '你忙的时候，它很安静',
        body: '与 WinIsland 同样的原生底座。小组件只在有东西要显示时才工作，不和你的应用抢资源。',
        items: [
          { label: '核心', value: 'Rust', body: '内存安全，保持轻量' },
          { label: '渲染', value: 'Skia', body: 'GPU 绘制玻璃与动画' },
          { label: '能效', value: '遮挡即暂停', body: '被完全挡住的小组件停止绘制' },
        ],
        details: [
          { title: '每屏一份捕获', body: '同一台显示器上的小组件共用一份桌面捕获。' },
          { title: '变化时才绘制', body: '内容不变就复用已有结果，液态玻璃的刷新率也有上限。' },
          { title: '整屏一起暂停', body: '一台显示器上的小组件全部被挡住时，捕获也会停下。' },
          { title: '闲置就释放', body: '暂时用不到的 GPU 资源会被自动释放。' },
        ],
      },
      closing: {
        title: 'WinWidget Desktop 即将到来',
        body: '关注仓库，第一时间看到它发布。',
      },
    },
    hero: {
      title: 'WinIsland，\n顺滑体验再升级',
      body: '活动状态一目了然，Windows实时活动新中心，崭新交互体验。',
      primary: '下载 Windows 版',
      secondary: '看看它如何工作',
      eyebrow: 'Windows 实时活动中心',
    },
    capabilities: ['实时歌词', '音频频谱', '媒体控制', 'Windows 通知', '音量与亮度', '资源占用', '隐私指示', '插件市场', '液态玻璃', '全屏自动隐藏'],
    showcase: {
      title: '走近这座小岛',
      body: '屏幕顶端小小的一块地方，装得下你每天最常看的东西。',
      items: [
        {
          label: '音乐',
          title: '封面、歌词与声音，一起律动',
          body: 'WinIsland 通过 Windows 媒体接口读取正在播放的内容，显示封面、歌曲信息和进度，播放控制就在手边。音频只在内存中分析，用来驱动频谱。',
          points: ['AMLL、LRCLIB、网易云、QQ、酷狗歌词', '本地 LRC 文件夹', '自选响应哪些应用'],
        },
        {
          label: '通知',
          title: '消息来了，瞥一眼就够',
          body: 'Windows 通知可以显示在灵动岛上，应用图标、标题和正文一目了然。这项功能默认关闭，开不开由你决定。',
          points: ['应用图标与标题', '默认关闭', '内容不离开本机'],
        },
        {
          label: '小组件',
          title: '常看的信息，点一下就到',
          body: '把时钟、日历和资源占用放进展开界面，CPU、内存、GPU、网络和磁盘都能随时掌握。插件还可以添加自己的小组件。',
          points: ['时钟与日历', '资源占用', '插件小组件'],
        },
        {
          label: '外观',
          title: '换一种风格，还是你的桌面',
          body: '在多种灵动岛风格之间切换，其中就有液态玻璃，字体也可以换成你喜欢的。游戏或视频全屏时，灵动岛可以自动让开。',
          points: ['多种灵动岛风格', '自定义字体', '全屏自动隐藏'],
        },
        {
          label: '细节',
          title: '系统没替你想到的小事',
          body: '音量键有了更清爽的提示；复制链接后，灵动岛会询问是否打开；麦克风或摄像头正在使用时，你能看得到。没有账号，没有广告，也没有遥测。',
          points: ['音量与亮度', '复制链接识别', '隐私指示'],
        },
      ],
    },
    glass: {
      title: '玻璃，是算出来的',
      body: 'Windows 没有现成的液态玻璃。WinIsland 捕获身后的桌面，用 GPU 着色器逐像素计算折射、色散和高光，让灵动岛真正融进你的壁纸。',
      points: ['折射率 1.45 的斯涅尔折射', '曲率连续的超椭圆圆角', '红绿蓝三通道色散'],
      action: '了解实现原理',
      imageAlt: 'Windows 桌面上的 WinIsland 灵动岛',
    },
    plugins: {
      title: '有了插件，能做的更多',
      body: '插件是通过稳定 ABI 加载的原生 DLL。可以从插件市场安装，也可以自己用 Rust 编写。',
      items: [
        { title: '状态文字', body: '在岛上发布一行文字，情况变化时更新或移除。' },
        { title: '媒体来源', body: '提供歌曲信息，并响应来自灵动岛的播放控制。' },
        { title: '自定义小组件', body: '在展开页的小组件网格里绘制自己的内容。' },
        { title: '设置页面', body: '为插件描述设置控件，并保存用户的选择。' },
        { title: '命令与快捷键', body: '注册命令，需要时再加上托盘入口和键盘快捷键。' },
        { title: '歌词处理', body: '在歌词显示之前，对解析好的每一行进行加工。' },
      ],
    },
    moments: {
      title: '实时交互，不错过每一刻精彩',
      body: '紧凑布局，容纳你的桌面。轻轻点击，信息与控制一览无余。',
      modes: {
        media: '媒体',
        notification: '通知',
        widgets: '小组件',
      },
    },
    features: {
      title: '从第一眼起，就像本该如此',
      items: [
        {
          title: '让音乐，跃然岛上',
          body: '专辑封面、播放控制、实时歌词和音频频谱，在一个流畅界面中自然汇合。',
          accent: 'coral',
        },
        {
          title: '每次交互，帧帧悦目',
          body: '数十次打磨的回弹动画，让每一次展开与收起都跟手、连贯，又不过分抢眼。',
          accent: 'blue',
        },
        {
          title: '你的桌面，你来做主',
          body: '自由选择小组件与外观。WinIsland 会适应你的桌面，而不是占据它。',
          accent: 'green',
        },
      ],
    },
    performance: {
      title: '轻量内核，不止炫酷',
      body: '原生界面，顶级内存占用优化，顺滑体验跃动指上。',
      items: [
        { label: '核心', value: 'Rust', body: '内存安全，保持轻量' },
        { label: '渲染', value: 'Skia', body: '硬件加速，动画顺滑' },
        { label: '平台', value: 'Windows', body: '原生媒体与系统接口' },
      ],
    },
    open: {
      title: '一座岛，千人千面',
      body: '阅读源码、参与插件系统设计，把每一个好想法变成现实。',
      source: '在 GitHub 查看',
      plugins: '插件开发指南',
    },
    download: {
      title: '让 WinIsland 登上你的桌面',
      body: '支持 Windows 10 2004 及以上版本与 Windows 11，仅支持 64 位系统。',
      stable: '下载最新正式版',
      nightly: '获取每日预览版',
      mirror: '镜像下载',
      nightlyHint: '预览版包含最新改动，但稳定性可能不如正式版。',
    },
    footer: {
      product: '产品',
      resources: '资源',
      community: '社区',
      license: '基于 GNU GPL v3 许可证发布。',
    },
    docs: {
      title: 'WinIsland 指南',
      subtitle: '安装、使用与扩展 WinIsland 所需的一切。',
      onThisPage: '文档目录',
      loadError: '文档加载失败，请刷新页面重试。',
      pages: {
        guide: '什么是 WinIsland？',
        'getting-started': '快速开始',
        download: '下载',
        'plugin-dev': '插件开发',
        'plugin-dev/quickstart': '插件快速开始',
        'plugin-dev/abi-lifecycle': 'ABI 与生命周期',
        'plugin-dev/services': '宿主服务',
        'plugin-dev/api': 'API 参考',
        'plugin-dev/api/context': 'Context API',
        'plugin-dev/api/activity': 'Activity API',
        'plugin-dev/api/system': 'System API',
        'plugin-dev/api/drawing': '绘制 SDK',
        'plugin-dev/api/media': 'Media API',
        'plugin-dev/api/i18n': 'I18n API',
        'plugin-dev/api/host-state': 'Host State API',
        'plugin-dev/api/widget': 'Widget API',
        'plugin-dev/api/surface': 'Surface API',
        'plugin-dev/api/input': 'Input API',
        'plugin-dev/api/events': 'Events API',
        'plugin-dev/api/command': 'Command API',
        'plugin-dev/api/media-session': 'Media Session API',
        'plugin-dev/api/lyrics-transform': 'Lyrics Transform API',
        'plugin-dev/api/settings': 'Settings API',
        'plugin-dev/api/text': 'Text API',
        'plugin-dev/api/image': 'Image API',
        'plugin-dev/api/store': 'Store API',
        'plugin-dev/api/log': 'Log API',
        'plugin-dev/packaging': '打包与安装',
        'api-changelog': 'API 更新日志',
        changelog: '更新日志',
      },
    },
    notFound: {
      title: '这座岛上什么也没有',
      body: '你访问的页面已移动或不存在。',
      action: '返回首页',
    },
  },
} as const

export const DOWNLOAD_RELEASE =
  'https://github.com/WinIslandProject/WinIsland/releases/latest/download/WinIsland-Setup.exe'
export const DOWNLOAD_NIGHTLY =
  'https://github.com/WinIslandProject/WinIsland/releases/download/nightly/WinIsland-Nightly-Setup.exe'
export const GITHUB_URL = 'https://github.com/WinIslandProject/WinIsland'
export const WINWIDGET_GITHUB_URL = 'https://github.com/WinIslandProject/WinWidget-Desktop'
