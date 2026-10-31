# خطة المشروع وسجل التنفيذ

> هذا الملف هو نقطة التسليم الأساسية لأي مطوّر أو وكيل AI يعمل على المستودع. اقرأه أولًا، ثم اقرأ المستندات المعمارية المرتبطة قبل تغيير التصميم.  
> آخر تحديث: 2026-10-31  
> الحالة العامة: **أساس المحرك وجسر Rust/Kotlin وبناء مكتبة Android ARM64 وتوليد Kotlin bindings مكتملة؛ اختبار APK وUniFFI على Android Emulator قيد التنفيذ. لا يوجد بعد تطبيق رسم أو Renderer إنتاجي أو اختبار قلم على جهاز فعلي.**

## 1. لماذا يوجد هذا الملف؟

يسجل هذا المستند ثلاثة أشياء منفصلة:

1. **الحقيقة الحالية:** ما يوجد فعلًا في `main`، مع روابط PR والالتزامات ونتائج الاختبارات.
2. **القرارات:** الاختيارات التقنية الحالية وأسبابها والتحفظات المرتبطة بها.
3. **الخطة:** المراحل المستقبلية، وترتيبها، وما الذي يثبت اكتمال كل مرحلة.

يجب تحديث قسم **سجل التنفيذ** وهذا الملف نفسه في كل PR ينجز مرحلة أو يغير قرارًا معماريًا. لا تكتب أن ميزة «مكتملة» لمجرد أن واجهتها أو مثالها بُني؛ اذكر بدقة ما تم بناؤه وما اختُبر وما بقي غير مثبت.

## 2. تعريف المنتج ونطاقه

**Rasm Studio** اسم عمل مؤقت لتطبيق محلي للرسم والتحريك ثنائي الأبعاد مع مسرح 2.5D.

محاور المنتج:

- **Canvas:** الرسم والفرش والطبقات والتحديد والتحويلات.
- **Timeline:** إطارات وcels وheld exposures وonion skin والتشغيل.
- **Stage:** دمج العمل ثنائي الأبعاد مع عناصر ومشاهد 3D حقيقية بشكل محدود ومدروس.

الأولوية هي تجربة رسم سريعة وموثوقة ثم التحريك frame-by-frame. لا نحاول بناء محرر فيديو عام أو برنامج نمذجة ثلاثية الأبعاد شامل.

### المنصات المستهدفة

- البداية المستهدفة: Android، مع أولوية للأجهزة اللوحية والقلم.
- المحرك: Rust مستقل عن الواجهة والمنصة قدر الإمكان.
- الواجهة: Kotlin + Jetpack Compose.
- لا يوجد Backend أو حساب سحابي مطلوب في الإصدار الأول؛ الملفات والرسم والحفظ محلية.

**تنبيه الحالة:** المستودع حاليًا مكتبات Rust تأسيسية. لا يحتوي بعد على تطبيق Android أو واجهة رسم أو محرر Timeline قابل للاستخدام.

## 3. خريطة المعمارية والقرارات الحالية

| المجال | القرار/المرشح الحالي | الحالة والتحفظ |
| --- | --- | --- |
| محرك الحسابات والمستند | Rust workspace | أساس أولي موجود، نموذج المستند الكامل غير موجود |
| واجهة Android | Kotlin + Compose | قرار معماري؛ لم يبدأ التطبيق بعد |
| ربط Rust مع Kotlin | UniFFI | مكتبة Android ARM64 وKotlin bindings مبنية في CI عبر Issue #14؛ اختبار الاستدعاء من APK داخل Emulator قيد التنفيذ في Issue #15 |
| GPU | wgpu | يظهر في تبعية تجربة Tileink؛ لا يوجد renderer إنتاجي |
| الرياضيات | glam + kurbo | مثبتان في workspace |
| بناء المسارات | lyon مرشح | لم يُدمج بعد |
| إدخال القلم والفرش | `rasm-engine-ink` | يولد عينات dabs بضغط متغير |
| Renderer تجريبي | Tileink 0.1.2 عبر adapter | Desktop spike فقط، وليس اختيارًا إنتاجيًا أو Android |
| صيغة المشروع | حزمة محلية بإصدارات schema وchunks | تصميم مستقبلي، لم يُنفذ |
| الحفظ والتراجع | معاملات/أوامر ونسخ tile عند الحاجة | تصميم مستقبلي، لم يُنفذ |

### قواعد معمارية يجب الحفاظ عليها

- لا تضع منطق الفرش أو المستند أو الرياضيات داخل واجهة Compose.
- لا تجعل `engine-core` يعتمد على أنواع GPU أو أنواع Tileink أو Android.
- لا تستخدم أنواع Tileink في صيغة الملف أو واجهة FFI.
- حافظ على فصل Document space وViewport space وPhysical pixel space وStage world space.
- استخدم التوقيت المطلق/monotonic حيث يلزم، وتجنب تراكم أخطاء float في timeline.
- اجعل أي اعتماد جديد خلف واجهة محددة، وقيّمه بالاختبارات والقياسات قبل اعتماده.
- لا تفترض دعم منصة لأن wgpu يدعمها نظريًا؛ اختبر مسار المكتبة والـ Surface ودورة الحياة على الجهاز نفسه.

## 4. سجل التنفيذ الفعلي

هذه قائمة بما تم دمجه في `main`، وليست قائمة نوايا.

### المرحلة 1: أساس المحرك والرياضيات وCI


- Issue: [#1](https://github.com/PanDuroDev/test/issues/1)
- PR: [#2](https://github.com/PanDuroDev/test/pull/2) — مدمج.
- PR: [#3](https://github.com/PanDuroDev/test/pull/3) — إعداد CI، مدمج.
- الالتزام الأساسي الناتج: `e03e707ef229a26795fa28cdb443357baf4b8ca8`.

**ما أُنجز:**

- إنشاء Rust workspace وcrate باسم `crates/engine-core`.
- تمثيل FPS ككسر rational وتحويل الزمن إلى إطارات بدقة دون الاعتماد على جمع float لكل إطار.
- تحويلات viewport ثنائية الأبعاد بدقة f64، مع عمليات pan/zoom/rotate وعمليات zoom anchored.
- إضافة One Euro filter للتنعيم.
- إضافة مستند الأساس المعماري `docs/architecture/FOUNDATION.md`.
- إعداد GitHub Actions لفحص التنسيق وClippy والاختبارات على Linux وWindows وmacOS.

**التحقق:** كانت فحوص الأنظمة الثلاثة ناجحة عند إكمال المرحلة. راجع سجل Actions والـ PR عند الحاجة إلى تفاصيل التشغيل.

**ما لم تنجزه المرحلة:** لا يوجد مستند رسم أو Renderer أو تطبيق Android؛ الموجود أساس رياضي واختبارات وCI.

### المرحلة 2: إدخال القلم والضغط وتجربة Tileink

- Issue: [#4](https://github.com/PanDuroDev/test/issues/4) — مغلق بعد الدمج.
- PR: [#5](https://github.com/PanDuroDev/test/pull/5) — مدمج.
- فرع التنفيذ: `ink/pressure-aware-strokes-tileink-spike`.
- رأس PR قبل الدمج: `68f9bfe5c58ab3012ef2cce2e92c7ce632ae6919`.
- التزام الدمج على `main`: `3f73104f4808fea39182bc0ce71472b35e64a56a`.
- [تشغيل CI](https://github.com/PanDuroDev/test/actions/runs/41234179449).

**ما أُنجز:**

1. إنشاء `crates/engine-ink` بحزمة `rasm-engine-ink`.
2. إضافة أنواع مستقلة عن GPU والمنصة:
   - `StrokeSample`: موضع f64، ضغط اختياري normalized، وتوقيت nanoseconds.
   - `BrushConfig`: نصف القطر الأساسي وحد الضغط الأدنى والأسّ والمسافة القصوى وعدد dabs.
   - `BrushDab`: موضع ونصف قطر وتوقيت.
   - `StrokeError`: أخطاء تحقق واضحة.
3. تنفيذ `tessellate_stroke()` لتوليد dabs موزعة حسب المسافة الهندسية، لا حسب عدد callbacks.
4. التحقق من الإحداثيات والضغط والتوقيت والإعدادات، ورفض العينات غير الصالحة، وفرض حد أقصى لعدد dabs.
5. الحفاظ على بداية الضربة ونهايتها، ودمج المواضع المتطابقة، وتغيير نصف القطر بمنحنى الضغط.
6. إنشاء `crates/renderer-tileink` بحزمة `rasm-renderer-tileink` كمحول تجريبي إلى Tileink Canvas.
7. إضافة مثال WGPU باسم `render_stroke` يحاول رسم ضربة نموذجية وتصدير PNG.
8. توثيق تقييم Tileink وحدود دعمه في `docs/architecture/RENDERER_SPIKE.md`.
9. تثبيت Tileink على الإصدار `0.1.2` ورفع MSRV إلى Rust 1.87 لمتطلبات WGPU 30.

**نتائج CI الموثقة للالتزام أعلاه:**

- Ubuntu: 14 اختبارًا في engine-core + 12 في engine-ink + 4 في renderer-tileink = **30 اختبارًا ناجحًا**.
- Windows: الاختبارات ناجحة.
- macOS: الاختبارات ناجحة.
- `cargo fmt --all -- --check`: ناجح.
- `cargo clippy --workspace --all-targets -- -D warnings`: ناجح.

**حدود يجب عدم إخفائها:**

- اختبارات CI لا تثبت أن المثال أنشأ صورة على GPU فعلي. المثال بُني ضمن فحوص all-targets، لكن تشغيله ورؤية الناتج لم يُتحقق منهما على جهاز رسومي فعلي.
- لم يحدث اختبار Android أو iOS.
- Tileink ليس Renderer إنتاجيًا معتمدًا. توثيق المشروع upstream لا يعلن دعم Android/iOS، وأنواع Canvas/Renderer غير Send/Sync.
- المحول الحالي يحول كل dab إلى SDF circle. هذا ليس بعد هندسة ضربة متصلة ذات caps/joins ومضاد تعرج مكتمل.
- لا يوجد بعد نظام tiles/damage خاص بالمستند أو نظام طبقات أو compositing إنتاجي.

## 5. الحالة الحالية للمستودع

عند استلام العمل، ابدأ من `main` وافحص الملفات الموجودة بدل الاعتماد على هذا الوصف وحده.

### الملفات الأساسية

- `Cargo.toml`: تعريف workspace والتبعيات والإصدارات المشتركة.
- `crates/engine-core/`: أساس الرياضيات والوقت والتحويلات والتنعيم.
- `crates/engine-ink/`: عينات القلم وتوليد dabs وهندسة الضربة المتصلة وحدود mesh.
- `crates/engine-tiles/`: تغطية البلاطات وتتبع البلاطات المتضررة على CPU.
- `crates/engine-ffi/`: DTO boundary عبر UniFFI، مع validation وحد أقصى لتغطية البلاطات.
- `tools/uniffi-bindgen/`: أداة توليد Kotlin bindings من metadata المكتبة.
- `android/ffi-smoke/`: تطبيق Android صغير لاختبار تحميل المكتبة واستدعاءات Rust من Kotlin داخل Emulator؛ ليس واجهة المنتج.
- `crates/renderer-tileink/`: محول Tileink التجريبي ومثال WGPU.
- `docs/architecture/FOUNDATION.md`: وثيقة التصميم والأساس الرياضي والمعماري.
- `docs/architecture/RENDERER_SPIKE.md`: تقييم Tileink وحدود اعتماده.
- `.github/workflows/ci.yml`: فحوص CI.

### ما يمكن فعله حاليًا

- استخدام مكتبة core للحسابات والتحويلات والتوقيت الموجودين.
- إرسال عينات Stroke صالحة إلى `tessellate_stroke()` والحصول على dabs، ثم بناء `StrokeMesh` والحصول على `StrokeBounds`.
- حساب تغطية مستطيل عبر `TileGrid` وتجميع البلاطات المتضررة في `DirtyTileSet` مع حدود موارد.
- تحويل dabs إلى Tileink Canvas في النموذج المكتبي التجريبي.
- تشغيل اختبارات workspace وفحوص التنسيق وClippy.

### ما لا يمكن الادعاء بوجوده بعد

- تطبيق Android قابل للاستخدام أو Compose product UI؛ يوجد فقط smoke host لاختبارات FFI.
- إدخال قلم Android متصل بالمحرك أو اختباره على جهاز فعلي.
- Renderer يعمل على Android Surface.
- ضربة فرشاة متصلة بجودة إنتاجية.
- دمج dirty regions المتجاورة أو invalidation graph للطبقات والتحويلات أو GPU tile cache.
- طبقات وصيغة ملفات وحفظ تلقائي وundo/redo.
- Timeline أو onion skin أو تشغيل رسوم.
- Stage ثلاثي الأبعاد أو Keyframes.

## 6. الخطة القادمة بالترتيب

المراحل أدناه **مخططة وليست منجزة**. يمكن تقسيمها إلى أكثر من Issue/PR إذا كبر نطاقها. لا تنتقل إلى المرحلة التالية لمجرد أن الكود «يعمل على جهازي»؛ استخدم معايير القبول.

### المرحلة 3: هندسة الضربة المتصلة وجودة الحواف — مكتملة

- Issue: [#8](https://github.com/PanDuroDev/test/issues/8) — مغلق.
- PR: [#10](https://github.com/PanDuroDev/test/pull/10) — مدمج.
- الالتزام الناتج: `b40141d4257f00d250e5b0f26a825cc8dc4ddd36`.

**ما أُنجز:**

- إضافة `StrokeMesh` و`StrokeVertex` و`StrokeGeometryConfig` و`StrokeGeometryError` في `rasm-engine-ink`.
- تحويل dab stream إلى triangle strip متصل، مع round caps، وتمثيل dab واحدة كقرص.
- استخدام miter محدود عند الزوايا الحادة، مع fallback للحالات القريبة من عكس الاتجاه.
- فحص ميزانية الرؤوس قبل التخصيص، وحد أقصى configurable، ومؤشرات 32-bit.
- رفض الرؤوس الناتجة غير المنتهية عند تجاوز الحسابات مجال f64، والاحتفاظ بالمثلثات الصغيرة غير المنعدمة.
- إضافة اختبارات للنهايات واتجاهها، winding، indices، تغيّر العرض بالضغط، الزوايا الحادة، near-reversal، overflow، tiny brushes وحد الذاكرة.

**التحقق الفعلي قبل الدمج:**

- 40 اختبارًا ناجحًا على Ubuntu.
- 40 اختبارًا ناجحًا على Windows.
- 40 اختبارًا ناجحًا على macOS.
- `cargo fmt --all -- --check` وClippy ناجحان.
- المثال الرسومي في Tileink لم يُشغّل على GPU فعلي، ولم يُختبر إخراج صورة.

**القيود المتبقية:** لا توجد حواف anti-alias fringe، ولا CPU reference raster/compositor، كما أن محول Tileink لا يزال يحول كل dab إلى SDF circle بدل استهلاك `StrokeMesh`. هذه الأعمال لم تُنفذ ضمن Issue #8.

### المرحلة 4: أساس نظام البلاطات والمناطق المتضررة — مكتمل عبر [PR #12](https://github.com/PanDuroDev/test/pull/12)

- Issue: [#11](https://github.com/PanDuroDev/test/issues/11) — مغلق.
- PR: [#12](https://github.com/PanDuroDev/test/pull/12) — مدمج.
- فرع التنفيذ: `render/tile-grid-dirty-tracking`.
- رأس PR قبل الدمج: `e65c7f315de443f673628e38bf382a549958b7e9`.
- التزام الدمج على `main`: `e3cafa2aa1bf6997c34892f6be85d04ac58ded2c`.
- [تشغيل CI](https://github.com/PanDuroDev/test/actions/runs/41239022031).

**ما أُنجز:**

- إضافة crate مستقلة `rasm-engine-tiles` دون اعتماد على Tileink أو WGPU.
- تمثيل `DocumentRect` بحدود half-open `[min, max)` بوحدات document raster pixels.
- تمثيل `TileCoord` بإحداثيات signed حتى تعمل المساحات السالبة بصورة صحيحة.
- تحويل المستطيلات إلى `TileCoverage` مضغوط، مع iterator row-major deterministic دون إنشاء قائمة tiles مسبقًا.
- دعم توسيع المستطيل بهامش finite non-negative.
- إضافة `DirtyTileSet` بحد أقصى configurable، وإدراج idempotent وترتيب ثابت.
- جعل عملية marking transactional: عند تجاوز حد المجموعة لا يحدث إدراج جزئي.
- رفض الإحداثيات غير المنتهية وخارج مجال i64، والمستطيلات المعكوسة، والهوامش غير الصالحة، ومناطق التغطية الكبيرة.

**التحقق الفعلي قبل الدمج:**

- Ubuntu: **59 اختبارًا ناجحًا** (14 core + 22 ink + 19 tiles + 4 renderer).
- macOS: **59 اختبارًا ناجحًا**.
- Windows: **59 اختبارًا ناجحًا**.
- `cargo fmt --all -- --check` و`cargo clippy --workspace --all-targets -- -D warnings` ناجحان.

**الحدود المتبقية وليست جزءًا من Issue #11:** لا يوجد دمج لمستطيلات dirty المتجاورة، ولا invalidation graph للطبقات أو التحويلات، ولا GPU texture cache، ولا benchmark يحدد حجم البلاطة الإنتاجي. حجم البلاطة يمرر كإعداد ولا نفترض أن 64 هو الحجم الأمثل. لا يثبت CI أن أي Renderer يعمل على GPU فعلي أو Android.

**تكامل الضربة والبلاطات:** أُنجز عبر [Issue #13](https://github.com/PanDuroDev/test/issues/13) و[PR #13](https://github.com/PanDuroDev/test/pull/13)، المدمج بالالتزام `2cea1784fcc601e06342be2786af1eba4904ef52`. توفر `StrokeMesh::bounds()` الآن `StrokeBounds` ليحوّلها المستهلك إلى `DocumentRect` وtile coverage دون إعادة حساب الحدود.

### المرحلة 5: إثبات مسار Android والقلم وGPU — قيد التنفيذ

**الهدف:** تقليل المخاطرة الأكبر مبكرًا: التأكد أن المحرك يمكن عرضه والتفاعل معه على جهاز Android حقيقي.

**الجزء الأول مكتمل: بناء جسر Rust/Kotlin**

- Issue: [#14](https://github.com/PanDuroDev/test/issues/14) — مغلق.
- PR: [#14](https://github.com/PanDuroDev/test/pull/14) — مدمج.
- التزام الدمج على `main`: `b70bbceca44a76d350cfbf70957fede125d49d50`.
- [تشغيل CI](https://github.com/PanDuroDev/test/actions/runs/41242558044) — ناجح بالكامل.
- إضافة `rasm-engine-ffi` كحد FFI مستقل باستخدام UniFFI 0.32.2.
- تمرير DTOs عددية للعينات، ثم استدعاء `engine-ink` لحساب mesh bounds و`engine-tiles` لحساب tile coverage بحد أعلى قدره 16,384 بلاطة لكل preview.
- بناء shared library لـ `aarch64-linux-android` عبر `cargo-ndk 4.1.2` وAndroid NDK `28.2.13676358` مع API level 23.
- توليد Kotlin bindings من metadata المكتبة في CI.
- التحقق من **67 اختبارًا ناجحًا على كل من Linux وWindows وmacOS**، ونجاح format وClippy.
- هذا يثبت cross-compilation وتوليد bindings، لكنه لم يثبت تشغيل Kotlin داخل APK أو Emulator عند اكتمال PR #14.

**الجزء الثاني قيد التنفيذ: اختبار APK وUniFFI داخل Emulator**

- Issue: [#15](https://github.com/PanDuroDev/test/issues/15) — مفتوح.
- Branch: `android/ffi-emulator-smoke-test`.
- إضافة تطبيق تجريبي صغير في `android/ffi-smoke`، وليس واجهة المنتج.
- إضافة UniFFI Kotlin configuration لتوليد الحزمة `com.pandurodev.rasm.ffi`.
- بناء مكتبة Rust لكل من `arm64-v8a` و`x86_64`، ودمجها في APK.
- إضافة JNA Android runtime وتوليد Kotlin bindings أثناء Gradle build.
- Instrumentation tests تستدعي `bridgeVersion()` و`previewStroke()`، وتتحقق من bounds وtiles ومن انتقال خطأ timestamp من Rust إلى Kotlin.
- CI سيبني Debug APK وAndroid test APK، ويتحقق من وجود المكتبتين داخل APK، ثم يشغل الاختبارات على Android x86_64 Emulator.
- إعداد المشروع يستهدف JDK 17 وGradle 9.5.0 وAGP 9.3.1 وKotlin Gradle Plugin 2.4.20 وNDK 28.2.13676358.

**ما لا يثبته هذا الجزء:** لا يضيف Compose UI للمنتج، ولا يلتقط MotionEvent أو يقيس الضغط/الميل، ولا ينشئ GPU Surface أو يقيس input-to-visible latency. نجاح Emulator لا يعادل اختبار جهاز فعلي.

**الجزء التالي بعد نجاح Issue #15:** التقاط MotionEvent التاريخية والضغط والميل والتوقيت ونوع المؤشر في تطبيق Android، ثم تمرير العينات على دفعات إلى Rust. بعد استقرار الإدخال نبدأ إثبات WGPU/Surface ودورة حياة Activity على جهاز ARM64 حقيقي.

**المهام العامة المتبقية للمرحلة:**

- اختبار دورة حياة Activity وإعادة إنشاء Surface وفقدان GPU context والذاكرة.
- فصل UI thread وinput queue وrender thread وIO worker بوضوح.
- قرار موثق بشأن الاستمرار مع Renderer مخصص مبني على wgpu. لا تعتمد Tileink على Android دون نجاح اختبار Surface فعلي.
- قياس أولي input-to-visible latency على جهاز موثق، لا على Emulator وحده.

**معايير قبول Android Surface النهائية:**

- بناء وتشغيل APK على جهاز Android ARM64 فعلي.
- ظهور الضربة على Surface، مع تحقق الضغط/الميل حيث يدعمه الجهاز.
- لا يحدث crash عند إعادة إنشاء Surface أو تدوير دورة الحياة المدعومة.
- توثيق GPU backend وقيود الجهاز وقياس input-to-visible latency.
- توثيق قرار Renderer وقيوده.

### المرحلة 6: نموذج المستند والطبقات والتراجع

**الهدف:** تحويل نتائج الرسم إلى مستند قابل للتحرير والحفظ.

**المهام المقترحة:**

- تعريف Document/Layer/Stroke/Brush resource IDs ونسخة schema.
- فصل بيانات الضربة الأصلية عن raster cache المشتق.
- تعريف عمليات تحرير transactional قابلة للتراجع والإعادة.
- وضع سياسة undo/redo للبيانات الثقيلة (tile-level copy-on-write أو snapshots).
- تصميم حزمة ملفات محلية تحتوي manifest وchunks للأصول والبلاطات.
- حفظ ذري، journal أو recovery mechanism، وترحيل schema بين الإصدارات.
- دراسة استيراد/تصدير OpenRaster دون اعتباره صيغة المشروع الأصلية.

**معايير القبول:**

- إنشاء مستند وحفظه وإغلاقه وإعادة فتحه دون فقد.
- round-trip tests لكل نوع بيانات مدعوم.
- اختبارات تعطل/انقطاع أثناء الحفظ أو محاكاة فشل كتابة.
- undo/redo يعيد الحالة المنطقية والبصرية الصحيحة.

### المرحلة 7: أول Canvas قابل للاستخدام

**الهدف:** دمج إدخال Android والمحرك والـ Renderer والمستند في شاشة رسم فعلية.

**المهام المقترحة:**

- Canvas pan/zoom/rotate.
- قلم وفرشاة أساسية وممحاة وألوان.
- layers raster أساسية.
- عرض البلاطات والتحديث الجزئي.
- مؤشر/معاينة الفرشاة قبل تثبيت الضربة.
- حفظ تلقائي واستعادة المستند.
- إظهار الأخطاء والحالات دون تجميد الواجهة.

**معايير القبول:**

- يمكن للمستخدم فتح لوحة، الرسم، تحريك العرض، إضافة طبقة، التراجع والإعادة، ثم حفظ المشروع واستعادته.
- لا تسقط عينات القلم بصمت تحت حمل الاختبار المحدد.
- benchmark قابل للتكرار مع توثيق الجهاز والدقة ومعدل التحديث.
- لا توجد أخطاء بصرية معروفة في اختبار جلسة رسم طويلة.

### المرحلة 8: تحريك Frame-by-frame

**الهدف:** إضافة workflow تحريك أساسي دون إدخال تعقيد keyframes مبكرًا.

**المهام المقترحة:**

- Tracks تحتوي cels.
- cels على إطار واحد أو held exposures.
- إنشاء/نسخ/حذف/تمديد الإطارات.
- onion skin للإطارات السابقة واللاحقة.
- تشغيل timeline باستخدام الوقت المطلق الموجود في engine-core.
- تصدير PNG sequence أو GIF بعد تثبيت playback.

**معايير القبول:**

- اختبارات frame-to-time وhold/exposure وحدود آخر إطار.
- playback ثابت دون انجراف زمني تراكمي.
- onion skin يحترم ترتيب الطبقات والشفافية.
- مشروع يحوي عدة cels يمكن حفظه وإعادة فتحه وتشغيله.

### المرحلة 9: مسرح 2.5D

**الهدف:** إدخال 3D حقيقي محدود دون تشتيت المنتج عن الرسم والتحريك.

**المهام المقترحة:**

- Scene world وكاميرا orthographic/perspective.
- عناصر أولية وملفات glTF عند الحاجة.
- وضع طبقات الرسم على planes أو إبقاؤها overlay.
- فصل depth/color passes عن compositing ثنائي الأبعاد.
- إضاءة وظلال بحدود واضحة.

**معايير القبول:**

- مشهد تجريبي موثق يظهر 2D و3D معًا.
- ترتيب compositing والشفافية والعمق يعمل كما هو محدد.
- لا ينهار أداء Canvas عند وجود عناصر Stage بسيطة.
- اختبار حفظ/فتح بيانات المشهد.

### المرحلة 10: Keyframes والتصدير والتحسين

**الهدف:** توسيع التحريك وتثبيت المنتج.

**المهام المقترحة:**

- Keyframe tracks للموضع والحجم والدوران والشفافية.
- hold وlinear وcubic Bezier interpolation.
- motion paths وeasing handles.
- تحسين cache والذاكرة والحفظ والتصدير.
- استيراد/تصدير بصيغ متفق عليها، وتوثيق حدود التوافق.
- مراجعة accessibility وتجربة القلم والأداء على أجهزة متعددة.

**معايير القبول:** اختبارات interpolation وserialization، ومشاهد benchmark مرجعية، وعدم كسر التوافق مع ملفات المشاريع السابقة دون migration.

## 7. طريقة العمل الإلزامية لكل مرحلة

لكل تغيير وظيفي أو معماري:

1. افتح Issue بنطاق واضح ومخرجات قابلة للاختبار.
2. أنشئ branch من آخر `main` نظيف.
3. نفذ أصغر تغيير مكتمل يمكن مراجعته.
4. أضف unit/integration tests ووثّق أي حدود أو قرارات.
5. افتح PR مرتبطًا بالـ Issue.
6. شغّل CI وانتظر نجاح Linux وWindows وmacOS وformat وClippy.
7. عالج أي فشل حقيقي ولا تتجاوز فحوصًا إلزامية.
8. حدّث هذا الملف في نفس PR: سجل ما تغير، رابط Issue/PR، الالتزام بعد الدمج عند توفره، الاختبارات والقيود وما بقي.
9. ادمج بعد نجاح الفحوص فقط.
10. تحقق من أن PR merged والـ Issue مغلق وأن `main` يحتوي النتيجة.

لا تدمج بناءً على نجاح منصة واحدة. لا تصف اختبارًا لم يُنفّذ على جهاز حقيقي بأنه ناجح.

## 8. أوامر التحقق القياسية

المتطلبات الحالية: Rust 1.87 أو أحدث، وCargo.

من جذر المستودع:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

تشغيل نموذج Tileink المكتبي يدويًا، على جهاز يملك GPU adapter صالحًا:

```sh
cargo run -p rasm-renderer-tileink --example render_stroke --release
```

المثال يتوقع كتابة الناتج إلى:

```text
target/renderer-spike.png
```

هذا الأمر اختياري حاليًا ولا يعوض اختبارات الوحدات. لا تعتبره متحققًا ما لم يُشغّل وتُفحص الصورة الناتجة فعلًا.

## 9. تعليمات مباشرة لوكيل AI أو مطوّر جديد

قبل كتابة الكود:

1. اقرأ هذا الملف كاملًا.
2. اقرأ `README.md` و`docs/architecture/FOUNDATION.md` و`docs/architecture/RENDERER_SPIKE.md`.
3. افحص آخر commit والـ Issues والـ PRs المفتوحة وحالة Actions على GitHub؛ قد يكون المستودع تقدم بعد تاريخ هذا السجل.
4. افحص محتوى الملفات الفعلي. هذا الملف دليل استلام، وليس بديلًا عن المصدر.
5. لا تعِد تنفيذ ميزة موجودة، ولا تفترض أن بندًا مخططًا أصبح منفذًا.
6. عند آخر تحديث، Issue #13 وPR #13 مدمجان لإضافة StrokeBounds، وIssue #14 وPR #14 مدمجان لإثبات بناء Android ARM64 وتوليد Kotlin bindings. Issue #15 مفتوح لاختبار APK من Kotlin إلى Rust داخل Emulator؛ لا تدّع نجاح الاختبار قبل اكتمال CI. بعده، انتقل إلى MotionEvent ثم Surface على جهاز فعلي.
7. أنشئ Issue/branch/PR واختبارات قبل الدمج.
8. حدّث هذا المستند في PR نفسه قبل الدمج.

عند تسليم العمل، اذكر دائمًا: آخر commit على main، آخر Issue/PR، ما تم اختباره فعلًا، ما لم يتم اختباره، والخطوة التالية المحددة.

## 10. سجل التغييرات لهذا المستند

- 2026-10-31: إنشاء خطة المشروع وسجل الاستلام بعد دمج PR #5. توثيق المرحلتين المنجزتين، حدود Tileink، نتائج CI، وخريطة المراحل 3–10.
- 2026-10-31: دمج ملف الاستمرارية عبر PR #7، الالتزام `60cb5cf94f6dc94113fb01b90e6f273cbc762832`.
- 2026-10-31: PR #9 أُغلق بعد إعادة تأسيس الفرع فوق main المحدث، واستُبدل بـ PR #10 لتجنب تضمين تغيير التوثيق المدمج مرة ثانية.
- 2026-10-31: دمج المرحلة 3 عبر PR #10 بالالتزام `b40141d4257f00d250e5b0f26a825cc8dc4ddd36` بعد نجاح 40 اختبارًا على كل نظام وفحوص format وClippy.
- 2026-10-31: بدء المرحلة 4 عبر Issue #11؛ إنشاء `rasm-engine-tiles` وإضافة grid coverage وtransactional dirty tile set على فرع منفصل.
- 2026-10-31: إكمال أساس المرحلة 4 ودمج PR #12 بالالتزام `e3cafa2aa1bf6997c34892f6be85d04ac58ded2c` بعد نجاح 59 اختبارًا على كل من Linux وWindows وmacOS، ونجاح format وClippy.
- 2026-10-31: دمج Issue #13 وPR #13 بالالتزام `2cea1784fcc601e06342be2786af1eba4904ef52` لإضافة `StrokeBounds` وواجهة `StrokeMesh::bounds()` وربط حدود هندسة الضربة بنظام البلاطات.
- 2026-10-31: دمج Issue #14 وPR #14 بالالتزام `b70bbceca44a76d350cfbf70957fede125d49d50` بعد نجاح بناء Android ARM64 وتوليد Kotlin bindings ونجاح 67 اختبارًا على Linux وWindows وmacOS، مع format وClippy.
- 2026-10-31: فتح Issue #15 والفرع `android/ffi-emulator-smoke-test` لإضافة APK smoke host وتشغيل UniFFI من Kotlin داخل Android Emulator.
