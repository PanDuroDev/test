# خطة المشروع وسجل التنفيذ

> هذا الملف هو نقطة التسليم الأساسية لأي مطوّر أو وكيل AI يعمل على المستودع. اقرأه أولًا، ثم اقرأ المستندات المعمارية المرتبطة قبل تغيير التصميم.  
> آخر تحديث: 2026-11-01  
> الحالة العامة: **المنتج PC Desktop فقط مع Windows كمنصة البداية. PR #18 أثبت نموذج النافذة والعرض على Windows، وPR #20 أضاف Document/Layer/Stroke وUndo/Redo، وPR #22 أضاف snapshots المتحققة وصيغة RASM v1، وPR #26 أضاف filesystem save/load ذريًا. CI #41258015343 نجح في الوظائف الست للمرحلة 6B2، وأُغلق Issue #25. اكتمل تنفيذ Issue #27 ودمج PR #28 بالالتزام `5f39d46c082aac621d91276f831c6d7e07e36fe8`، وأُغلق Issue #27. أصبح إدخال الرسم مرتبطًا بنموذج Document وHistory، مع document space وتحويل viewport يحافظ على نسبة الأبعاد واختصارات Undo/Redo. نجح CI #41260460992 و#41260977695 في الوظائف الست، بما فيها اختبارات Windows/Linux/macOS وFormat/Clippy وAndroid FFI وWindows prototype artifact. لا يوجد حتى الآن تطبيق رسم متكامل أو Renderer إنتاجي أو Timeline أو واجهة أدوات نهائية. لم يُختبر قلم مخصص أو تُسجل قياسات latency/ذاكرة. Android/Kotlin FFI محفوظ كتجربة مستقلة وليس هدف المنتج.**

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

- المنصة المستهدفة: تطبيق PC Desktop فقط، مع Windows كمنصة المنتج الأولى.
- لا يوجد تطبيق Android أو iOS ضمن نطاق المنتج.
- المحرك: Rust مستقل عن الواجهة والمنصة قدر الإمكان.
- طبقة النافذة والإدخال والواجهة المكتبية لم تُحسم بعد؛ يجب اختيارها بعد نموذج Windows يقيس القلم وGPU وتكامل الواجهة.
- لا يوجد Backend أو حساب سحابي مطلوب في الإصدار الأول؛ الملفات والرسم والحفظ محلية.

**تنبيه الحالة:** المستودع حاليًا مكتبات Rust تأسيسية وتجارب تكامل. لا يحتوي بعد على تطبيق Desktop أو واجهة رسم أو محرر Timeline قابل للاستخدام.

## 3. خريطة المعمارية والقرارات الحالية

| المجال | القرار/المرشح الحالي | الحالة والتحفظ |
| --- | --- | --- |
| محرك الحسابات والمستند | Rust workspace | أساس أولي موجود، نموذج المستند الكامل غير موجود |
| واجهة Desktop | لم يُحسم الإطار بعد | Windows-first؛ القرار ينتظر نموذجًا يقيس إدخال القلم وwgpu وسطح الرسم |
| إدخال Windows | Windows Pointer APIs مرشح | يجب إثبات الضغط والميل وسجل العينات والكمون على Windows فعلي |
| ربط Rust مع Kotlin | UniFFI تجريبي | PR #14 مدمج وPR #15 يضيف smoke host منفصلًا؛ لا يمثل واجهة المنتج ولا يعتمد عليه تطبيق Desktop |
| GPU | wgpu | يظهر في تبعية تجربة Tileink؛ لا يوجد renderer إنتاجي |
| الرياضيات | glam + kurbo | مثبتان في workspace |
| بناء المسارات | lyon مرشح | لم يُدمج بعد |
| إدخال القلم والفرش | `rasm-engine-ink` | يولد عينات dabs بضغط متغير |
| Renderer تجريبي | Tileink 0.1.2 عبر adapter | Desktop spike فقط، وليس اختيارًا إنتاجيًا أو Android |
| صيغة المشروع | حزمة محلية بإصدارات schema وchunks | تصميم مستقبلي، لم يُنفذ |
| الحفظ والتراجع | معاملات/أوامر ونسخ tile عند الحاجة | تصميم مستقبلي، لم يُنفذ |

### قواعد معمارية يجب الحفاظ عليها

- لا تضع منطق الفرش أو المستند أو الرياضيات داخل طبقة الواجهة المكتبية.
- لا تجعل `engine-core` يعتمد على أنواع GPU أو أنواع Tileink أو تفاصيل Windows/Android.
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
- `crates/engine-ffi/`: DTO boundary عبر UniFFI، محفوظ كتجربة تكامل منفصلة.
- `tools/uniffi-bindgen/`: أداة توليد Kotlin bindings من metadata المكتبة.
- `android/ffi-smoke/`: اختبار Android مستقل لجسر FFI، وليس تطبيق المنتج.
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

- تطبيق Desktop منتجي قابل للاستخدام أو واجهة أدوات نهائية؛ الموجود نموذج تكامل مستقل في PR #18 وليس تطبيق المنتج.
- تحقق فعلي من Windows pen/pointer أو GPU على جهاز فعلي؛ الكود موجود واجتاز CI، لكن اختبار الأجهزة لم يحدث.
- Renderer إنتاجي؛ نموذج WGPU في PR #18 مسار أولي وليس Renderer نهائيًا.
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

### تجربة تكامل FFI السابقة: Android/Kotlin — محفوظة ومنفصلة عن هدف المنتج

هذه تجربة تحقق لحدود Rust/Kotlin فقط، وليست مسار بناء تطبيق المنتج.

**الحالة:**
- PR: [#14](https://github.com/PanDuroDev/test/pull/14) — مدمج.
- PR: [#15](https://github.com/PanDuroDev/test/pull/15) — مفتوح؛ يضيف Android smoke host ويختبر تحميل المكتبة واستدعاء Rust من Kotlin على x86_64 Emulator.
- PR #14 أضاف `rasm-engine-ffi` باستخدام UniFFI 0.32.2، وDTOs عددية للعينات والحدود والبلاطات، وبناء Android ARM64 وتوليد Kotlin bindings.
- PR #15 يبني APK ويحتوي على مكتبات ARM64 وx86_64. في تشغيل CI #49 نجح بناء APK، والتحقق من المكتبتين، وإقلاع Emulator بعد تفعيل KVM، وتشغيل **3 instrumentation tests بنجاح**.
- أضيف تمكين KVM في GitHub Actions وتحسين مدخلات Gradle لتتبع ملفات workspace ومولد UniFFI؛ نجحت بعدها جميع فحوص Rust وFormat وClippy والاختبارات الثلاثة على Emulator في CI #49.

**حدود التجربة:**
- لا تثبت إدخال قلم فعلي أو عرض GPU أو وجود واجهة منتج.
- لا تتطلب من تطبيق Desktop استخدام Kotlin أو UniFFI.
- تبقى ملفات Android والـ FFI محفوظة؛ لا تُحذف عند اعتماد PC-only.

### المرحلة 5B: إثبات مسار Windows Desktop والنافذة وGPU — مكتملة عبر [Issue #17](https://github.com/PanDuroDev/test/issues/17) و[PR #18](https://github.com/PanDuroDev/test/pull/18)

**الفرع:** `desktop/windows-renderer-prototype` — دُمج PR #18 في `main` بالالتزام `4dec1f49861949a37572f01d05f8dceaecfa2ec3`.

**ما أُنجز:**
- نافذة Desktop وسطح WGPU باستخدام `winit 0.30.13` و`wgpu 30.0.0`.
- إرسال `StrokeMesh` الحالي إلى مسار العرض باستخدام GPU vertex/index buffers.
- تكامل Windows hook لرسائل `WM_POINTERDOWN/UPDATE/UP` وقراءة التاريخ عبر `GetPointerPenInfoHistory`، مع الضغط والميل والدوران عندما يوفرها النظام.
- fallback للرسم بالفأرة، ومعالجة resize وفقدان/إعادة تهيئة Surface.
- بناء EXE Release ورفعه كـ Artifact باسم `rasm-desktop-prototype-windows-x64`.

**التحقق:**
- [CI #41253986206](https://github.com/PanDuroDev/test/actions/runs/41253986206): نجحت الوظائف الست، بما فيها Windows Release artifact.
- [CI #41254738784](https://github.com/PanDuroDev/test/actions/runs/41254738784): نجحت الوظائف الست على رأس PR النهائي.
- المستخدم شغّل EXE على Windows وأكد نجاح اختبارات التشغيل المتاحة: ظهور النافذة والخط التجريبي، الرسم بالفأرة، وتغيير/استعادة النافذة.
- تم تنزيل ZIP وفحص محتوياته وترويسة PE للملف التنفيذي.

**حدود لا تزال قائمة:** لم يُجرَ اختبار قلم مخصص يثبت pressure/tilt، ولا توجد قياسات موثقة لـ input-to-visible latency أو CPU/GPU/memory. لا يعني نجاح النموذج اعتماد UI framework أو renderer إنتاجي. هذه البنود تبقى ضمن تقييم renderer/input اللاحق، ولا تمنع اعتماد إثبات النافذة والعرض الأساسي.

### المرحلة 6A: نموذج المستند والطبقات وسجل التعديلات — مكتملة عبر [Issue #19](https://github.com/PanDuroDev/test/issues/19) و[PR #20](https://github.com/PanDuroDev/test/pull/20)

**الالتزام على main:** `904d0c80271d7371a8474c4c92e2b5248ecc847e`.

**ما أُنجز:**
- crate مستقلة `rasm-engine-document` تعتمد على `engine-ink` و`glam` فقط، ولا تعتمد على UI أو GPU أو Windows أو Tileink.
- `Document` بأبعاد raster غير صفرية وطبقة ابتدائية واحدة.
- `Layer` بمعرف ثابت واسم ورؤية وopacity وترتيب bottom-to-top.
- `Stroke` يخزن عينات الإدخال الأصلية و`BrushConfig`، وليس `StrokeMesh` مشتقًا.
- عمليات إضافة/حذف الطبقات، إعادة التسمية، الرؤية، opacity، وإعادة الترتيب.
- عمليات إضافة/حذف الضربات مع حفظ معرفاتها ومواضعها عند Undo/Redo.
- سجل أوامر عكسي محدود افتراضيًا إلى 256 عملية، وحد مليون sample للضربة الواحدة، وميزانية snapshot مقدارها 2 مليون sample و100 ألف stroke و2048 layer.
- API للتحويل إلى DocumentSnapshot وإعادة البناء مع التحقق من الأبعاد والمعرفات والأسماء والفرش والعينات وترتيب التوقيت.

**التحقق الفعلي:** [CI #41255940489](https://github.com/PanDuroDev/test/actions/runs/41255940489) نجح في الوظائف الست: **87 اختبارًا على كل نظام تشغيل** (Linux وWindows وmacOS)، وformat وClippy وبناء Android ARM64 وتوليد Kotlin bindings، وبناء/رفع Windows prototype artifact. أُغلق Issue #19 بعد الدمج.

**حدود المرحلة:** لا يوجد ملف `.rasm` أو serialization أو save/load أو autosave أو crash recovery. snapshot API لا تنفذ I/O؛ التخزين الفعلي يبدأ في المرحلة التالية.

### المرحلة 6B1: snapshots وصيغة ملف .rasm — مكتملة عبر [Issue #21](https://github.com/PanDuroDev/test/issues/21) و[PR #22](https://github.com/PanDuroDev/test/pull/22)

**فرع التنفيذ:** `storage/versioned-document-format`.
**التزام الدمج على main:** `52e9e118bc826862c1a8e76449c7aa59c8acec55`.
**CI:** [#41257206578](https://github.com/PanDuroDev/test/actions/runs/41257206578) — نجحت الوظائف الست.

**ما أُنجز:**
- getters لإعدادات `BrushConfig` حتى تحفظ إعدادات الفرشاة وتستعاد عبر constructor المتحقق.
- `DocumentSnapshot` وطبقات/ضربات/عينات/Brush snapshots عامة مستقلة عن serde وGPU.
- `Document::snapshot()` و`Document::from_snapshot()` مع التحقق من IDs وduplicates والحدود وبيانات العينات.
- فرض حدود الموارد أيضًا أثناء التعديلات العادية: 2048 طبقة، 100 ألف ضربة، ومليونا sample إجمالًا.
- crate `rasm-engine-storage` لصيغة container ثابتة: magic `RASM`، file version، flags، payload length، CRC32، ثم JSON payload يحمل schema version مستقلًا.
- رفض الملفات المبتورة، trailing bytes، payload الأكبر من 256 MiB، checksum غير المطابق، flags أو versions غير المدعومة، والـ JSON أو المستند غير الصالح.
- Round-trip يحافظ على IDs وترتيب الطبقات وإعدادات الفرش والعينات الأصلية.

**التحقق:** Format وClippy واختبارات Workspace على Linux وWindows وmacOS وبناء Android ARM64 وتوليد Kotlin وWindows prototype artifact، كلها نجحت في CI #41257206578.

**حدود المرحلة:** codec يتعامل مع byte buffers فقط. لا يوجد filesystem I/O أو حفظ ذري أو autosave أو journal أو crash recovery. CRC32 يكشف التلف العرضي ولا يوفر حماية تشفيرية ضد التلاعب المتعمد.

### المرحلة 6B2: القراءة والكتابة الذرية لملفات RASM — مكتملة عبر [Issue #25](https://github.com/PanDuroDev/test/issues/25) و[PR #26](https://github.com/PanDuroDev/test/pull/26)

**فرع التنفيذ:** `storage/atomic-file-save-load`.
**رأس PR:** `cd9c6b9ca4485f622215c8324d2134d9bae0cfeb`.
**CI:** [#41258015343](https://github.com/PanDuroDev/test/actions/runs/41258015343) — نجحت الوظائف الست، بما فيها اختبارات Windows وLinux وmacOS وFormat وClippy وAndroid ARM64 FFI وWindows prototype artifact.

**ما أُنجز:**
- إضافة `save_atomic(path, document)` و`load(path)` إلى `rasm-engine-storage`.
- التحقق من حجم الملف عبر metadata قبل القراءة، ثم فرض حد فعلي أثناء القراءة لمنع تجاوز الحد عند تغير الملف.
- إنشاء ملف مؤقت فريد في مجلد الوجهة نفسه عبر `create_new` لمنع التصادم أو الكتابة فوق ملف مؤقت قائم.
- كتابة المحتوى كاملًا ثم flush و`sync_all` قبل بدء الاستبدال.
- Windows: استبدال عبر `MoveFileExW` باستخدام `MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH`.
- Unix: `rename` داخل filesystem نفسه، ثم مزامنة مجلد الوجهة بعد commit.
- حذف الملف المؤقت عند فشل الكتابة أو الاستبدال قبل commit، مع اختبار يثبت بقاء bytes النسخة السابقة.
- اختبارات round-trip وreplace وملف تالف وفشل commit وتنظيف temp file.

**التحفظات والحدود:**
- نجاح replace/rename يضمن ذرية الظهور وفق سلوك filesystem المستخدم، لكنه ليس ضمانًا مطلقًا ضد كل فقد للطاقة أو عيوب التخزين.
- فشل directory sync على Unix بعد rename يُبلغ كخطأ `DirectorySyncAfterCommit`؛ قد يكون الملف الجديد أصبح ظاهرًا بالفعل ولا يصح افتراض بقاء القديم.
- لا تشمل المرحلة autosave أو journaling أو crash recovery أو تكامل الواجهة.
- Artifact `rasm-desktop-prototype-windows-x64` أُنتج بنجاح في CI، لكن ذلك لا يثبت تشغيل نسخة EXE الجديدة يدويًا على جهاز المستخدم.

**النتيجة:** أساس حفظ وفتح ملفات RASM موجود الآن في المحرك؛ المرحلة التالية هي بناء مسار Canvas Desktop قابل للاستخدام وربطه بالمستند والحفظ، لا إعادة تنفيذ codec أو filesystem I/O.

### المرحلة 7: أول Canvas Desktop قابل للاستخدام

**الهدف:** دمج إدخال Windows والمحرك والـ Renderer والمستند في شاشة رسم فعلية.

**المهام المقترحة:**

- التقاط Windows pen/pointer history والضغط والميل والتوقيت ونوع المؤشر عبر adapter مكتبي.
- تمرير العينات إلى Rust عبر queue/دفعات مع منع إسقاطها بصمت.

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

### المرحلة 7A: ربط Canvas Desktop بنموذج المستند وUndo/Redo — مكتملة عبر [PR #28](https://github.com/PanDuroDev/test/pull/28) (أُغلق [Issue #27](https://github.com/PanDuroDev/test/issues/27))

**فرع التنفيذ:** `desktop/document-backed-canvas`.

**النطاق:**
- استبدال قائمة الضربات المكتملة المؤقتة بحالة `CanvasDocument` تحتوي `Document` و`History`.
- إبقاء الضربة النشطة كـ preview مؤقت، ثم حفظها في طبقة المستند عند اكتمال الإدخال.
- استخدام `ViewportTransform` لفصل document space عن physical window pixels.
- fitting للوحة 1920×1080 مع letterboxing والحفاظ على نسبة الأبعاد عند تغيير حجم النافذة.
- تحويل mouse وWindows pen إلى document coordinates، وقص العرض إلى حدود اللوحة.
- Undo/Redo عبر Ctrl+Z وCtrl+Y وCtrl+Shift+Z.
- renderer التجريبي يقرأ الضربات المكتملة من Document ويعيد بناء mesh حاليًا عند كل redraw.

**العمل المنفذ:** أضيفت `apps/desktop-prototype/src/canvas.rs` مع حالة المستند وviewport mapping واختباراتها؛ رُبط `main.rs` بالمستند والـ history؛ أضيفت اختصارات Undo/Redo واختبارات دمج الضربات في المستند. يقرأ renderer الضربات من طبقات Document بدل مخزن الضربات المكتملة القديم.

**التحقق:** [CI #41260460992](https://github.com/PanDuroDev/test/actions/runs/41260460992) و[CI #41260977695](https://github.com/PanDuroDev/test/actions/runs/41260977695) نجحا في الوظائف الست: اختبارات Linux وWindows وmacOS، Format وClippy، Android ARM64 FFI، وبناء ورفع Windows prototype artifact (`rasm-desktop-prototype-windows-x64`, artifact ID `13065571623`). دُمج PR #28 بالالتزام `5f39d46c082aac621d91276f831c6d7e07e36fe8` وأُغلق Issue #27. لا توجد ادعاءات ببناء محلي.

**الحدود:** لا توجد بعد لوحة أدوات أو واجهة إدارة طبقات أو dialogs للفتح والحفظ. لا يزال renderer spike ولا يطبق compositing كاملًا لقيم opacity. لا توجد قياسات أداء أو اختبار قلم فعلي جديد. المرحلة مدمجة في `main`. بقي تشغيل artifact على Windows للتحقق البصري بعد التغييرات، واختبار قلم فعلي لقياس pressure/tilt.

### المرحلة 7B1: التحكم في عرض اللوحة (pan/zoom) — مرتبطة بـ[Issue #29](https://github.com/PanDuroDev/test/issues/29)

**فرع التنفيذ:** `desktop/canvas-pan-zoom`.

**النطاق:**
- إضافة `ViewState` في `canvas.rs`: معامل تكبير بين 0.25× و8× وإزاحة pan فوق fit المحافظ على نسبة الأبعاد.
- تكبير بعجلة الفأرة حول موضع المؤشر مع بقاء النقطة تحت المؤشر ثابتة.
- تحريك اللوحة بسحب الزر الأوسط، و`Ctrl+0` لإعادة العرض إلى fit.
- الحفاظ على zoom/pan عند تغيير حجم النافذة.
- تصفير `ModifiersState` وإنهاء pan عند فقدان تركيز النافذة (إصلاح «Ctrl العالق»).

**العمل المنفذ:** `CanvasViewport::fit_view` يطبّق `ViewState` فوق fit، و`CanvasDocument` يحتفظ بأبعاد النافذة لإعادة بناء أساس fit عند zoom/pan/resize. أضيفت اختبارات headless في `canvas.rs` لتثبات النقطة تحت المؤشر، وحدود التكبير، وإزاحة pan، وإعادة الضبط، والحفاظ على التكبير بعد resize، ورُبطت الحوادث في `main.rs`.

**التحقق:** [CI 41263713391](https://github.com/PanDuroDev/test/actions/runs/41263713391) — نجحت الوظائف الست: fmt وclippy، اختبارات Linux/Windows/macOS، بناء FFI لـAndroid، وبناء Windows prototype artifact. لم يُجرَ أي بناء أو تشغيل محلي؛ التحقق كله عبر CI.

**الحدود:** لا يوجد دوران للوحة ولا لوحة أدوات ولا إدارة طبقات ولا «fit to selection»؛ حدود التكبير ثابتة بين 0.25× و8×.


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
6. الحالة الحالية: PR #18 و#20 و#22 و#26 و#28 مدمجة؛ Windows prototype، نموذج المستند وUndo/Redo، snapshots وRASM v1، filesystem save/load الذري، وربط Desktop بالمستند وviewport mapping موجودة في main. أُغلق Issue #25 بواسطة PR #26 وIssue #27 بواسطة PR #28. اجتاز PR #28 جميع وظائف CI الست في التشغيلين #41260460992 و#41260977695. لا يزال التطبيق الكامل وواجهة الرسم الإنتاجية وRenderer الإنتاجي وTimeline غير موجودة. قلم Windows المخصص وقياسات الأداء لم تُثبت. تجربة Android/Kotlin FFI عبر PR #14/#15 محفوظة كاختبار تكامل منفصل ولا تُحذف أو تُجعل شرطًا للمنتج.
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
- 2026-10-31: فتح Issue #13 لإضافة `StrokeBounds` وواجهة `StrokeMesh::bounds()` تمهيدًا لربط هندسة الضربة بمناطق إعادة الرسم.
- 2026-10-31: دمج PR #14 لإضافة جسر UniFFI DTO وبناء Android ARM64 وتوليد Kotlin bindings.
- 2026-10-31: فتح PR #15 لإضافة Android FFI smoke host؛ البناء والتغليف نجحا، لكن Emulator لم يكمل الإقلاع في أول تشغيل CI. بقي PR مفتوحًا كتجربة تكامل مستقلة.
- 2026-11-01: دمج PR #16 لتثبيت هدف المنتج PC Desktop فقط مع Windows أولًا، وحفظ كود Android/UniFFI كتجربة مستقلة.
- 2026-11-01: بدء Issue #17 على فرع `desktop/windows-renderer-prototype`: إضافة نافذة winit، وWGPU surface، وعرض StrokeMesh، وربط Windows pen/pointer history. التنفيذ لم يُثبت بعد عبر CI أو تشغيل فعلي على Windows.
- 2026-11-01: فتح Issue #17 لتنفيذ نموذج Windows window + wgpu surface + pen/pointer input قبل اعتماد إطار الواجهة.
- 2026-11-01: اكتمل CI لـ PR #18، مع نجاح format وClippy واختبارات Linux/Windows/macOS وبناء Android ARM64 FFI في التشغيل #41252760860.
- 2026-11-01: اكتمل تشغيل CI #41253986206 بنجاح في الوظائف الست. أُنشئ Artifact `rasm-desktop-prototype-windows-x64` (EXE + README) بحجم مضغوط 2.75 MB وصلاحية حتى 2026-11-14؛ تم تنزيل ZIP وفحص محتوياته وترويسة Windows PE، دون الادعاء بتشغيل EXE على جهاز Windows فعلي.
- 2026-11-01: أكد المستخدم نجاح تشغيل Windows prototype والاختبارات الوظيفية المتاحة؛ دُمج PR #18 بالالتزام `4dec1f49861949a37572f01d05f8dceaecfa2ec3` وأُغلق Issue #17. لم تُسجل قياسات أداء أو تجربة قلم مخصص.
- 2026-11-01: فُتح Issue #19 وبدأ فرع `core/document-model-undo-redo` لتنفيذ المرحلة 6A: Document/Layer/Stroke model وbounded command-based Undo/Redo؛ serialization والحفظ مؤجلان للمرحلة 6B.
- 2026-11-01: دمج PR #20 بالالتزام `904d0c80271d7371a8474c4c92e2b5248ecc847e` وإغلاق Issue #19. اجتاز CI #41255940489 جميع الوظائف الست، بما فيها 87 اختبارًا على كل من Linux وWindows وmacOS، Format وClippy وبناء Android ARM64/Kotlin bindings وWindows artifact.
- 2026-11-01: فتح Issue #21 وبدء فرع `storage/versioned-document-format` لتنفيذ DocumentSnapshot validated API وversioned `.rasm` container مع CRC32 وJSON schema v1. لم يبدأ filesystem I/O أو atomic save بعد.

- 2026-11-01: دمج PR #22 بالالتزام `52e9e118bc826862c1a8e76449c7aa59c8acec55` وإغلاق Issue #21. نجحت وظائف CI الست في التشغيل #41257206578؛ أصبحت snapshots المتحققة وصيغة RASM v1 متاحتين في main.
- 2026-11-01: فتح Issue #25 وبدء فرع `storage/atomic-file-save-load` لتنفيذ save/load ذري باستخدام temporary file في مجلد الوجهة، مع اختبارات حماية الملف السابق عند فشل commit.
- 2026-11-01: اكتمل تنفيذ المرحلة 6B2 عبر PR #26: `save_atomic` و`load` مع الاستبدال الذري على Windows وUnix، وحدود قراءة صارمة واختبارات فشل commit وتنظيف الملفات المؤقتة. نجحت الوظائف الست في CI #41258015343، وأُنتج Windows prototype artifact. لم يُثبت تشغيل EXE الجديد يدويًا، ولا يشمل هذا العمل autosave أو journal أو crash recovery.

- 2026-11-01: فُتح Issue #27 وفرع `desktop/document-backed-canvas` لربط prototype بنموذج Document/History، وفصل document coordinates عن بكسلات النافذة، وإضافة Undo/Redo عبر اختصارات لوحة المفاتيح. أضيفت حالة Canvas وviewport mapping واختباراتها، ثم رُبط إدخال mouse/Windows pen والعارض بالمستند. اكتمل CI #41260460992 و#41260977695 بجميع الوظائف الست بما فيها Windows prototype artifact؛ دُمج PR #28 بالالتزام `5f39d46c082aac621d91276f831c6d7e07e36fe8` وأُغلق Issue #27.
- 2026-11-01: فُتح Issue #29 وفرع `desktop/canvas-pan-zoom` لإضافة zoom/pan للوحة وتصفير modifiers عند فقدان التركيز، وحفظ العرض بعد resize؛ نجح CI في جميع وظائفه ([CI 41263713391](https://github.com/PanDuroDev/test/actions/runs/41263713391)) والـPR رقم #30.
- 2026-11-01: إصلاح حدود اللوحة (الحد الأيمن/السفلي صار غير شامل)، وإضافة ذاكرة tessellation للضربات المكتملة بدل إعادة بنائها كل إطار، وإظهار أخطاء رفض الضربة في شريط العنوان بدل `stderr` فقط (PR #34).
