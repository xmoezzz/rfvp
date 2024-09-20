fn assert_round_trips(s: &str) {
    let mut res = Vec::new();
    write_sjis_string(s, &mut res).unwrap();
    let s2 = read_sjis_string(&mut io::Cursor::new(&res), Some(res.len())).unwrap();
    assert_eq!(s, &s2);
}

// Instead of checking the concrete pairs of Shift-JIS bytes and unicode characters we check whether we can round-trip some of the unicode chars
// this is done because the encoding table used by the game is a bit messy
// 1. It uses '・' for a lot of unmapped cells, making its encoding non-unique (the canonical way to encode it is b"\x81\x45")
// 2. Some characters in the JIS vendor-specific area are duplicated (between rows 89-92 and 115-119)
// this makes it a bit cumbersome to prepare tests for those...
// so I just decided to test round trips =)

#[test]
fn test_sjis_round_trip_ascii() {
    assert_round_trips("!'#$%&\"()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~\x7f");
}

#[test]
fn test_sjis_round_trip_katakana() {
    assert_round_trips("\\u{F8F0}｡｢｣､･ｦｧｨｩｪｫｬｭｮｯｰｱｲｳｴｵｶｷｸｹｺｻｼｽｾｿﾀﾁﾂﾃﾄﾅﾆﾇﾈﾉﾊﾋﾌﾍﾎﾏﾐﾑﾒﾓﾔﾕﾖﾗﾘﾙﾚﾛﾜﾝﾞﾟ");
}

#[test]
fn test_sjis_round_trip_row_1() {
    assert_round_trips("\u{3000}、。，．・：；？！゛゜´｀¨＾￣＿ヽヾゝゞ〃仝々〆〇ー―‐／＼～∥｜…‥‘’“”（）〔〕［］｛｝〈〉《》「」『』【】＋－±×÷＝≠＜＞≦≧∞∴♂♀°′″℃￥＄￠￡％＃＆＊＠§☆★○●◎◇",);
}
#[test]
fn test_sjis_round_trip_row_2() {
    assert_round_trips("◆□■△▲▽▼※〒→←↑↓〓・・・・・・・・・・・∈∋⊆⊇⊂⊃∪∩・・・・・・・・∧∨￢⇒⇔∀∃・・・・・・・・・・・∠⊥⌒∂∇≡≒≪≫√∽∝∵∫∬・・・・・・・Å‰♯♭♪†‡¶・・・・◯");
}
#[test]
fn test_sjis_round_trip_row_3() {
    assert_round_trips("・・・・・・・・・・・・・・・０１２３４５６７８９・・・・・・・ＡＢＣＤＥＦＧＨＩＪＫＬＭＮＯＰＱＲＳＴＵＶＷＸＹＺ・・・・・・ａｂｃｄｅｆｇｈｉｊｋｌｍｎｏｐｑｒｓｔｕｖｗｘｙｚ・・・・");
}
#[test]
fn test_sjis_round_trip_row_4() {
    assert_round_trips("ぁあぃいぅうぇえぉおかがきぎくぐけげこごさざしじすずせぜそぞただちぢっつづてでとどなにぬねのはばぱひびぴふぶぷへべぺほぼぽまみむめもゃやゅゆょよらりるれろゎわゐゑをん・・・・・・・・・・・");
}
#[test]
fn test_sjis_round_trip_row_5() {
    assert_round_trips("ァアィイゥウェエォオカガキギクグケゲコゴサザシジスズセゼソゾタダチヂッツヅテデトドナニヌネノハバパヒビピフブプヘベペホボポマミムメモャヤュユョヨラリルレロヮワヰヱヲンヴヵヶ・・・・・・・・");
}
#[test]
fn test_sjis_round_trip_row_6() {
    assert_round_trips("ΑΒΓΔΕΖΗΘΙΚΛΜΝΞΟΠΡΣΤΥΦΧΨΩ・・・・・・・・αβγδεζηθικλμνξοπρστυφχψω・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・");
}
#[test]
fn test_sjis_round_trip_row_7() {
    assert_round_trips("АБВГДЕЁЖЗИЙКЛМНОПРСТУФХЦЧШЩЪЫЬЭЮЯ・・・・・・・・・・・・・・・абвгдеёжзийклмнопрстуфхцчшщъыьэюя・・・・・・・・・・・・・");
}
#[test]
fn test_sjis_round_trip_row_8() {
    assert_round_trips("─│┌┐┘└├┬┤┴┼━┃┏┓┛┗┣┳┫┻╋┠┯┨┷┿┝┰┥┸╂・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・");
}
#[test]
fn test_sjis_round_trip_row_9() {
    assert_round_trips("・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・");
}
#[test]
fn test_sjis_round_trip_row_10() {
    assert_round_trips("・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・");
}
#[test]
fn test_sjis_round_trip_row_11() {
    assert_round_trips("・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・");
}
#[test]
fn test_sjis_round_trip_row_12() {
    assert_round_trips("・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・");
}
#[test]
fn test_sjis_round_trip_row_13() {
    assert_round_trips("①②③④⑤⑥⑦⑧⑨⑩⑪⑫⑬⑭⑮⑯⑰⑱⑲⑳ⅠⅡⅢⅣⅤⅥⅦⅧⅨⅩ・㍉㌔㌢㍍㌘㌧㌃㌶㍑㍗㌍㌦㌣㌫㍊㌻㎜㎝㎞㎎㎏㏄㎡・・・・・・・・㍻〝〟№㏍℡㊤㊥㊦㊧㊨㈱㈲㈹㍾㍽㍼≒≡∫∮∑√⊥∠∟⊿∵∩∪・・");
}
#[test]
fn test_sjis_round_trip_row_14() {
    assert_round_trips("・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・");
}
#[test]
fn test_sjis_round_trip_row_15() {
    assert_round_trips("・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・");
}
#[test]
fn test_sjis_round_trip_row_16() {
    assert_round_trips("亜唖娃阿哀愛挨姶逢葵茜穐悪握渥旭葦芦鯵梓圧斡扱宛姐虻飴絢綾鮎或粟袷安庵按暗案闇鞍杏以伊位依偉囲夷委威尉惟意慰易椅為畏異移維緯胃萎衣謂違遺医井亥域育郁磯一壱溢逸稲茨芋鰯允印咽員因姻引飲淫胤蔭");
}
#[test]
fn test_sjis_round_trip_row_17() {
    assert_round_trips("院陰隠韻吋右宇烏羽迂雨卯鵜窺丑碓臼渦嘘唄欝蔚鰻姥厩浦瓜閏噂云運雲荏餌叡営嬰影映曳栄永泳洩瑛盈穎頴英衛詠鋭液疫益駅悦謁越閲榎厭円園堰奄宴延怨掩援沿演炎焔煙燕猿縁艶苑薗遠鉛鴛塩於汚甥凹央奥往応");
}
#[test]
fn test_sjis_round_trip_row_18() {
    assert_round_trips("押旺横欧殴王翁襖鴬鴎黄岡沖荻億屋憶臆桶牡乙俺卸恩温穏音下化仮何伽価佳加可嘉夏嫁家寡科暇果架歌河火珂禍禾稼箇花苛茄荷華菓蝦課嘩貨迦過霞蚊俄峨我牙画臥芽蛾賀雅餓駕介会解回塊壊廻快怪悔恢懐戒拐改");
}
#[test]
fn test_sjis_round_trip_row_19() {
    assert_round_trips("魁晦械海灰界皆絵芥蟹開階貝凱劾外咳害崖慨概涯碍蓋街該鎧骸浬馨蛙垣柿蛎鈎劃嚇各廓拡撹格核殻獲確穫覚角赫較郭閣隔革学岳楽額顎掛笠樫橿梶鰍潟割喝恰括活渇滑葛褐轄且鰹叶椛樺鞄株兜竃蒲釜鎌噛鴨栢茅萱");
}
#[test]
fn test_sjis_round_trip_row_20() {
    assert_round_trips("粥刈苅瓦乾侃冠寒刊勘勧巻喚堪姦完官寛干幹患感慣憾換敢柑桓棺款歓汗漢澗潅環甘監看竿管簡緩缶翰肝艦莞観諌貫還鑑間閑関陥韓館舘丸含岸巌玩癌眼岩翫贋雁頑顔願企伎危喜器基奇嬉寄岐希幾忌揮机旗既期棋棄");
}
#[test]
fn test_sjis_round_trip_row_21() {
    assert_round_trips("機帰毅気汽畿祈季稀紀徽規記貴起軌輝飢騎鬼亀偽儀妓宜戯技擬欺犠疑祇義蟻誼議掬菊鞠吉吃喫桔橘詰砧杵黍却客脚虐逆丘久仇休及吸宮弓急救朽求汲泣灸球究窮笈級糾給旧牛去居巨拒拠挙渠虚許距鋸漁禦魚亨享京");
}
#[test]
fn test_sjis_round_trip_row_22() {
    assert_round_trips("供侠僑兇競共凶協匡卿叫喬境峡強彊怯恐恭挟教橋況狂狭矯胸脅興蕎郷鏡響饗驚仰凝尭暁業局曲極玉桐粁僅勤均巾錦斤欣欽琴禁禽筋緊芹菌衿襟謹近金吟銀九倶句区狗玖矩苦躯駆駈駒具愚虞喰空偶寓遇隅串櫛釧屑屈");
}
#[test]
fn test_sjis_round_trip_row_23() {
    assert_round_trips("掘窟沓靴轡窪熊隈粂栗繰桑鍬勲君薫訓群軍郡卦袈祁係傾刑兄啓圭珪型契形径恵慶慧憩掲携敬景桂渓畦稽系経継繋罫茎荊蛍計詣警軽頚鶏芸迎鯨劇戟撃激隙桁傑欠決潔穴結血訣月件倹倦健兼券剣喧圏堅嫌建憲懸拳捲");
}
#[test]
fn test_sjis_round_trip_row_24() {
    assert_round_trips("検権牽犬献研硯絹県肩見謙賢軒遣鍵険顕験鹸元原厳幻弦減源玄現絃舷言諺限乎個古呼固姑孤己庫弧戸故枯湖狐糊袴股胡菰虎誇跨鈷雇顧鼓五互伍午呉吾娯後御悟梧檎瑚碁語誤護醐乞鯉交佼侯候倖光公功効勾厚口向");
}
#[test]
fn test_sjis_round_trip_row_25() {
    assert_round_trips("后喉坑垢好孔孝宏工巧巷幸広庚康弘恒慌抗拘控攻昂晃更杭校梗構江洪浩港溝甲皇硬稿糠紅紘絞綱耕考肯肱腔膏航荒行衡講貢購郊酵鉱砿鋼閤降項香高鴻剛劫号合壕拷濠豪轟麹克刻告国穀酷鵠黒獄漉腰甑忽惚骨狛込");
}
#[test]
fn test_sjis_round_trip_row_26() {
    assert_round_trips("此頃今困坤墾婚恨懇昏昆根梱混痕紺艮魂些佐叉唆嵯左差査沙瑳砂詐鎖裟坐座挫債催再最哉塞妻宰彩才採栽歳済災采犀砕砦祭斎細菜裁載際剤在材罪財冴坂阪堺榊肴咲崎埼碕鷺作削咋搾昨朔柵窄策索錯桜鮭笹匙冊刷");
}
#[test]
fn test_sjis_round_trip_row_27() {
    assert_round_trips("察拶撮擦札殺薩雑皐鯖捌錆鮫皿晒三傘参山惨撒散桟燦珊産算纂蚕讃賛酸餐斬暫残仕仔伺使刺司史嗣四士始姉姿子屍市師志思指支孜斯施旨枝止死氏獅祉私糸紙紫肢脂至視詞詩試誌諮資賜雌飼歯事似侍児字寺慈持時");
}
#[test]
fn test_sjis_round_trip_row_28() {
    assert_round_trips("次滋治爾璽痔磁示而耳自蒔辞汐鹿式識鴫竺軸宍雫七叱執失嫉室悉湿漆疾質実蔀篠偲柴芝屡蕊縞舎写射捨赦斜煮社紗者謝車遮蛇邪借勺尺杓灼爵酌釈錫若寂弱惹主取守手朱殊狩珠種腫趣酒首儒受呪寿授樹綬需囚収周");
}
#[test]
fn test_sjis_round_trip_row_29() {
    assert_round_trips("宗就州修愁拾洲秀秋終繍習臭舟蒐衆襲讐蹴輯週酋酬集醜什住充十従戎柔汁渋獣縦重銃叔夙宿淑祝縮粛塾熟出術述俊峻春瞬竣舜駿准循旬楯殉淳準潤盾純巡遵醇順処初所暑曙渚庶緒署書薯藷諸助叙女序徐恕鋤除傷償");
}
#[test]
fn test_sjis_round_trip_row_30() {
    assert_round_trips("勝匠升召哨商唱嘗奨妾娼宵将小少尚庄床廠彰承抄招掌捷昇昌昭晶松梢樟樵沼消渉湘焼焦照症省硝礁祥称章笑粧紹肖菖蒋蕉衝裳訟証詔詳象賞醤鉦鍾鐘障鞘上丈丞乗冗剰城場壌嬢常情擾条杖浄状畳穣蒸譲醸錠嘱埴飾");
}
#[test]
fn test_sjis_round_trip_row_31() {
    assert_round_trips("拭植殖燭織職色触食蝕辱尻伸信侵唇娠寝審心慎振新晋森榛浸深申疹真神秦紳臣芯薪親診身辛進針震人仁刃塵壬尋甚尽腎訊迅陣靭笥諏須酢図厨逗吹垂帥推水炊睡粋翠衰遂酔錐錘随瑞髄崇嵩数枢趨雛据杉椙菅頗雀裾");
}
#[test]
fn test_sjis_round_trip_row_32() {
    assert_round_trips("澄摺寸世瀬畝是凄制勢姓征性成政整星晴棲栖正清牲生盛精聖声製西誠誓請逝醒青静斉税脆隻席惜戚斥昔析石積籍績脊責赤跡蹟碩切拙接摂折設窃節説雪絶舌蝉仙先千占宣専尖川戦扇撰栓栴泉浅洗染潜煎煽旋穿箭線");
}
#[test]
fn test_sjis_round_trip_row_33() {
    assert_round_trips("繊羨腺舛船薦詮賎践選遷銭銑閃鮮前善漸然全禅繕膳糎噌塑岨措曾曽楚狙疏疎礎祖租粗素組蘇訴阻遡鼠僧創双叢倉喪壮奏爽宋層匝惣想捜掃挿掻操早曹巣槍槽漕燥争痩相窓糟総綜聡草荘葬蒼藻装走送遭鎗霜騒像増憎");
}
#[test]
fn test_sjis_round_trip_row_34() {
    assert_round_trips("臓蔵贈造促側則即息捉束測足速俗属賊族続卒袖其揃存孫尊損村遜他多太汰詑唾堕妥惰打柁舵楕陀駄騨体堆対耐岱帯待怠態戴替泰滞胎腿苔袋貸退逮隊黛鯛代台大第醍題鷹滝瀧卓啄宅托択拓沢濯琢託鐸濁諾茸凧蛸只");
}
#[test]
fn test_sjis_round_trip_row_35() {
    assert_round_trips("叩但達辰奪脱巽竪辿棚谷狸鱈樽誰丹単嘆坦担探旦歎淡湛炭短端箪綻耽胆蛋誕鍛団壇弾断暖檀段男談値知地弛恥智池痴稚置致蜘遅馳築畜竹筑蓄逐秩窒茶嫡着中仲宙忠抽昼柱注虫衷註酎鋳駐樗瀦猪苧著貯丁兆凋喋寵");
}
#[test]
fn test_sjis_round_trip_row_36() {
    assert_round_trips("帖帳庁弔張彫徴懲挑暢朝潮牒町眺聴脹腸蝶調諜超跳銚長頂鳥勅捗直朕沈珍賃鎮陳津墜椎槌追鎚痛通塚栂掴槻佃漬柘辻蔦綴鍔椿潰坪壷嬬紬爪吊釣鶴亭低停偵剃貞呈堤定帝底庭廷弟悌抵挺提梯汀碇禎程締艇訂諦蹄逓");
}
#[test]
fn test_sjis_round_trip_row_37() {
    assert_round_trips("邸鄭釘鼎泥摘擢敵滴的笛適鏑溺哲徹撤轍迭鉄典填天展店添纏甜貼転顛点伝殿澱田電兎吐堵塗妬屠徒斗杜渡登菟賭途都鍍砥砺努度土奴怒倒党冬凍刀唐塔塘套宕島嶋悼投搭東桃梼棟盗淘湯涛灯燈当痘祷等答筒糖統到");
}
#[test]
fn test_sjis_round_trip_row_38() {
    assert_round_trips("董蕩藤討謄豆踏逃透鐙陶頭騰闘働動同堂導憧撞洞瞳童胴萄道銅峠鴇匿得徳涜特督禿篤毒独読栃橡凸突椴届鳶苫寅酉瀞噸屯惇敦沌豚遁頓呑曇鈍奈那内乍凪薙謎灘捺鍋楢馴縄畷南楠軟難汝二尼弐迩匂賑肉虹廿日乳入");
}
#[test]
fn test_sjis_round_trip_row_39() {
    assert_round_trips("如尿韮任妊忍認濡禰祢寧葱猫熱年念捻撚燃粘乃廼之埜嚢悩濃納能脳膿農覗蚤巴把播覇杷波派琶破婆罵芭馬俳廃拝排敗杯盃牌背肺輩配倍培媒梅楳煤狽買売賠陪這蝿秤矧萩伯剥博拍柏泊白箔粕舶薄迫曝漠爆縛莫駁麦");
}
#[test]
fn test_sjis_round_trip_row_40() {
    assert_round_trips("函箱硲箸肇筈櫨幡肌畑畠八鉢溌発醗髪伐罰抜筏閥鳩噺塙蛤隼伴判半反叛帆搬斑板氾汎版犯班畔繁般藩販範釆煩頒飯挽晩番盤磐蕃蛮匪卑否妃庇彼悲扉批披斐比泌疲皮碑秘緋罷肥被誹費避非飛樋簸備尾微枇毘琵眉美");
}
#[test]
fn test_sjis_round_trip_row_41() {
    assert_round_trips("鼻柊稗匹疋髭彦膝菱肘弼必畢筆逼桧姫媛紐百謬俵彪標氷漂瓢票表評豹廟描病秒苗錨鋲蒜蛭鰭品彬斌浜瀕貧賓頻敏瓶不付埠夫婦富冨布府怖扶敷斧普浮父符腐膚芙譜負賦赴阜附侮撫武舞葡蕪部封楓風葺蕗伏副復幅服");
}
#[test]
fn test_sjis_round_trip_row_42() {
    assert_round_trips("福腹複覆淵弗払沸仏物鮒分吻噴墳憤扮焚奮粉糞紛雰文聞丙併兵塀幣平弊柄並蔽閉陛米頁僻壁癖碧別瞥蔑箆偏変片篇編辺返遍便勉娩弁鞭保舗鋪圃捕歩甫補輔穂募墓慕戊暮母簿菩倣俸包呆報奉宝峰峯崩庖抱捧放方朋");
}
#[test]
fn test_sjis_round_trip_row_43() {
    assert_round_trips("法泡烹砲縫胞芳萌蓬蜂褒訪豊邦鋒飽鳳鵬乏亡傍剖坊妨帽忘忙房暴望某棒冒紡肪膨謀貌貿鉾防吠頬北僕卜墨撲朴牧睦穆釦勃没殆堀幌奔本翻凡盆摩磨魔麻埋妹昧枚毎哩槙幕膜枕鮪柾鱒桝亦俣又抹末沫迄侭繭麿万慢満");
}
#[test]
fn test_sjis_round_trip_row_44() {
    assert_round_trips("漫蔓味未魅巳箕岬密蜜湊蓑稔脈妙粍民眠務夢無牟矛霧鵡椋婿娘冥名命明盟迷銘鳴姪牝滅免棉綿緬面麺摸模茂妄孟毛猛盲網耗蒙儲木黙目杢勿餅尤戻籾貰問悶紋門匁也冶夜爺耶野弥矢厄役約薬訳躍靖柳薮鑓愉愈油癒");
}
#[test]
fn test_sjis_round_trip_row_45() {
    assert_round_trips("諭輸唯佑優勇友宥幽悠憂揖有柚湧涌猶猷由祐裕誘遊邑郵雄融夕予余与誉輿預傭幼妖容庸揚揺擁曜楊様洋溶熔用窯羊耀葉蓉要謡踊遥陽養慾抑欲沃浴翌翼淀羅螺裸来莱頼雷洛絡落酪乱卵嵐欄濫藍蘭覧利吏履李梨理璃");
}
#[test]
fn test_sjis_round_trip_row_46() {
    assert_round_trips("痢裏裡里離陸律率立葎掠略劉流溜琉留硫粒隆竜龍侶慮旅虜了亮僚両凌寮料梁涼猟療瞭稜糧良諒遼量陵領力緑倫厘林淋燐琳臨輪隣鱗麟瑠塁涙累類令伶例冷励嶺怜玲礼苓鈴隷零霊麗齢暦歴列劣烈裂廉恋憐漣煉簾練聯");
}
#[test]
fn test_sjis_round_trip_row_47() {
    assert_round_trips("蓮連錬呂魯櫓炉賂路露労婁廊弄朗楼榔浪漏牢狼篭老聾蝋郎六麓禄肋録論倭和話歪賄脇惑枠鷲亙亘鰐詫藁蕨椀湾碗腕・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・");
}
#[test]
fn test_sjis_round_trip_row_48() {
    assert_round_trips("弌丐丕个丱丶丼丿乂乖乘亂亅豫亊舒弍于亞亟亠亢亰亳亶从仍仄仆仂仗仞仭仟价伉佚估佛佝佗佇佶侈侏侘佻佩佰侑佯來侖儘俔俟俎俘俛俑俚俐俤俥倚倨倔倪倥倅伜俶倡倩倬俾俯們倆偃假會偕偐偈做偖偬偸傀傚傅傴傲");
}
#[test]
fn test_sjis_round_trip_row_49() {
    assert_round_trips("僉僊傳僂僖僞僥僭僣僮價僵儉儁儂儖儕儔儚儡儺儷儼儻儿兀兒兌兔兢竸兩兪兮冀冂囘册冉冏冑冓冕冖冤冦冢冩冪冫决冱冲冰况冽凅凉凛几處凩凭凰凵凾刄刋刔刎刧刪刮刳刹剏剄剋剌剞剔剪剴剩剳剿剽劍劔劒剱劈劑辨");
}
#[test]
fn test_sjis_round_trip_row_50() {
    assert_round_trips("辧劬劭劼劵勁勍勗勞勣勦飭勠勳勵勸勹匆匈甸匍匐匏匕匚匣匯匱匳匸區卆卅丗卉卍凖卞卩卮夘卻卷厂厖厠厦厥厮厰厶參簒雙叟曼燮叮叨叭叺吁吽呀听吭吼吮吶吩吝呎咏呵咎呟呱呷呰咒呻咀呶咄咐咆哇咢咸咥咬哄哈咨");
}
#[test]
fn test_sjis_round_trip_row_51() {
    assert_round_trips("咫哂咤咾咼哘哥哦唏唔哽哮哭哺哢唹啀啣啌售啜啅啖啗唸唳啝喙喀咯喊喟啻啾喘喞單啼喃喩喇喨嗚嗅嗟嗄嗜嗤嗔嘔嗷嘖嗾嗽嘛嗹噎噐營嘴嘶嘲嘸噫噤嘯噬噪嚆嚀嚊嚠嚔嚏嚥嚮嚶嚴囂嚼囁囃囀囈囎囑囓囗囮囹圀囿圄圉");
}
#[test]
fn test_sjis_round_trip_row_52() {
    assert_round_trips("圈國圍圓團圖嗇圜圦圷圸坎圻址坏坩埀垈坡坿垉垓垠垳垤垪垰埃埆埔埒埓堊埖埣堋堙堝塲堡塢塋塰毀塒堽塹墅墹墟墫墺壞墻墸墮壅壓壑壗壙壘壥壜壤壟壯壺壹壻壼壽夂夊夐夛梦夥夬夭夲夸夾竒奕奐奎奚奘奢奠奧奬奩");
}
#[test]
fn test_sjis_round_trip_row_53() {
    assert_round_trips("奸妁妝佞侫妣妲姆姨姜妍姙姚娥娟娑娜娉娚婀婬婉娵娶婢婪媚媼媾嫋嫂媽嫣嫗嫦嫩嫖嫺嫻嬌嬋嬖嬲嫐嬪嬶嬾孃孅孀孑孕孚孛孥孩孰孳孵學斈孺宀它宦宸寃寇寉寔寐寤實寢寞寥寫寰寶寳尅將專對尓尠尢尨尸尹屁屆屎屓");
}
#[test]
fn test_sjis_round_trip_row_54() {
    assert_round_trips("屐屏孱屬屮乢屶屹岌岑岔妛岫岻岶岼岷峅岾峇峙峩峽峺峭嶌峪崋崕崗嵜崟崛崑崔崢崚崙崘嵌嵒嵎嵋嵬嵳嵶嶇嶄嶂嶢嶝嶬嶮嶽嶐嶷嶼巉巍巓巒巖巛巫已巵帋帚帙帑帛帶帷幄幃幀幎幗幔幟幢幤幇幵并幺麼广庠廁廂廈廐廏");
}
#[test]
fn test_sjis_round_trip_row_55() {
    assert_round_trips("廖廣廝廚廛廢廡廨廩廬廱廳廰廴廸廾弃弉彝彜弋弑弖弩弭弸彁彈彌彎弯彑彖彗彙彡彭彳彷徃徂彿徊很徑徇從徙徘徠徨徭徼忖忻忤忸忱忝悳忿怡恠怙怐怩怎怱怛怕怫怦怏怺恚恁恪恷恟恊恆恍恣恃恤恂恬恫恙悁悍惧悃悚");
}
#[test]
fn test_sjis_round_trip_row_56() {
    assert_round_trips("悄悛悖悗悒悧悋惡悸惠惓悴忰悽惆悵惘慍愕愆惶惷愀惴惺愃愡惻惱愍愎慇愾愨愧慊愿愼愬愴愽慂慄慳慷慘慙慚慫慴慯慥慱慟慝慓慵憙憖憇憬憔憚憊憑憫憮懌懊應懷懈懃懆憺懋罹懍懦懣懶懺懴懿懽懼懾戀戈戉戍戌戔戛");
}
#[test]
fn test_sjis_round_trip_row_57() {
    assert_round_trips("戞戡截戮戰戲戳扁扎扞扣扛扠扨扼抂抉找抒抓抖拔抃抔拗拑抻拏拿拆擔拈拜拌拊拂拇抛拉挌拮拱挧挂挈拯拵捐挾捍搜捏掖掎掀掫捶掣掏掉掟掵捫捩掾揩揀揆揣揉插揶揄搖搴搆搓搦搶攝搗搨搏摧摯摶摎攪撕撓撥撩撈撼");
}
#[test]
fn test_sjis_round_trip_row_58() {
    assert_round_trips("據擒擅擇撻擘擂擱擧舉擠擡抬擣擯攬擶擴擲擺攀擽攘攜攅攤攣攫攴攵攷收攸畋效敖敕敍敘敞敝敲數斂斃變斛斟斫斷旃旆旁旄旌旒旛旙无旡旱杲昊昃旻杳昵昶昴昜晏晄晉晁晞晝晤晧晨晟晢晰暃暈暎暉暄暘暝曁暹曉暾暼");
}
#[test]
fn test_sjis_round_trip_row_59() {
    assert_round_trips("曄暸曖曚曠昿曦曩曰曵曷朏朖朞朦朧霸朮朿朶杁朸朷杆杞杠杙杣杤枉杰枩杼杪枌枋枦枡枅枷柯枴柬枳柩枸柤柞柝柢柮枹柎柆柧檜栞框栩桀桍栲桎梳栫桙档桷桿梟梏梭梔條梛梃檮梹桴梵梠梺椏梍桾椁棊椈棘椢椦棡椌棍");
}
#[test]
fn test_sjis_round_trip_row_60() {
    assert_round_trips("棔棧棕椶椒椄棗棣椥棹棠棯椨椪椚椣椡棆楹楷楜楸楫楔楾楮椹楴椽楙椰楡楞楝榁楪榲榮槐榿槁槓榾槎寨槊槝榻槃榧樮榑榠榜榕榴槞槨樂樛槿權槹槲槧樅榱樞槭樔槫樊樒櫁樣樓橄樌橲樶橸橇橢橙橦橈樸樢檐檍檠檄檢檣");
}
#[test]
fn test_sjis_round_trip_row_61() {
    assert_round_trips("檗蘗檻櫃櫂檸檳檬櫞櫑櫟檪櫚櫪櫻欅蘖櫺欒欖鬱欟欸欷盜欹飮歇歃歉歐歙歔歛歟歡歸歹歿殀殄殃殍殘殕殞殤殪殫殯殲殱殳殷殼毆毋毓毟毬毫毳毯麾氈氓气氛氤氣汞汕汢汪沂沍沚沁沛汾汨汳沒沐泄泱泓沽泗泅泝沮沱沾");
}
#[test]
fn test_sjis_round_trip_row_62() {
    assert_round_trips("沺泛泯泙泪洟衍洶洫洽洸洙洵洳洒洌浣涓浤浚浹浙涎涕濤涅淹渕渊涵淇淦涸淆淬淞淌淨淒淅淺淙淤淕淪淮渭湮渮渙湲湟渾渣湫渫湶湍渟湃渺湎渤滿渝游溂溪溘滉溷滓溽溯滄溲滔滕溏溥滂溟潁漑灌滬滸滾漿滲漱滯漲滌");
}
#[test]
fn test_sjis_round_trip_row_63() {
    assert_round_trips("漾漓滷澆潺潸澁澀潯潛濳潭澂潼潘澎澑濂潦澳澣澡澤澹濆澪濟濕濬濔濘濱濮濛瀉瀋濺瀑瀁瀏濾瀛瀚潴瀝瀘瀟瀰瀾瀲灑灣炙炒炯烱炬炸炳炮烟烋烝烙焉烽焜焙煥煕熈煦煢煌煖煬熏燻熄熕熨熬燗熹熾燒燉燔燎燠燬燧燵燼");
}
#[test]
fn test_sjis_round_trip_row_64() {
    assert_round_trips("燹燿爍爐爛爨爭爬爰爲爻爼爿牀牆牋牘牴牾犂犁犇犒犖犢犧犹犲狃狆狄狎狒狢狠狡狹狷倏猗猊猜猖猝猴猯猩猥猾獎獏默獗獪獨獰獸獵獻獺珈玳珎玻珀珥珮珞璢琅瑯琥珸琲琺瑕琿瑟瑙瑁瑜瑩瑰瑣瑪瑶瑾璋璞璧瓊瓏瓔珱");
}
#[test]
fn test_sjis_round_trip_row_65() {
    assert_round_trips("瓠瓣瓧瓩瓮瓲瓰瓱瓸瓷甄甃甅甌甎甍甕甓甞甦甬甼畄畍畊畉畛畆畚畩畤畧畫畭畸當疆疇畴疊疉疂疔疚疝疥疣痂疳痃疵疽疸疼疱痍痊痒痙痣痞痾痿痼瘁痰痺痲痳瘋瘍瘉瘟瘧瘠瘡瘢瘤瘴瘰瘻癇癈癆癜癘癡癢癨癩癪癧癬癰");
}
#[test]
fn test_sjis_round_trip_row_66() {
    assert_round_trips("癲癶癸發皀皃皈皋皎皖皓皙皚皰皴皸皹皺盂盍盖盒盞盡盥盧盪蘯盻眈眇眄眩眤眞眥眦眛眷眸睇睚睨睫睛睥睿睾睹瞎瞋瞑瞠瞞瞰瞶瞹瞿瞼瞽瞻矇矍矗矚矜矣矮矼砌砒礦砠礪硅碎硴碆硼碚碌碣碵碪碯磑磆磋磔碾碼磅磊磬");
}
#[test]
fn test_sjis_round_trip_row_67() {
    assert_round_trips("磧磚磽磴礇礒礑礙礬礫祀祠祗祟祚祕祓祺祿禊禝禧齋禪禮禳禹禺秉秕秧秬秡秣稈稍稘稙稠稟禀稱稻稾稷穃穗穉穡穢穩龝穰穹穽窈窗窕窘窖窩竈窰窶竅竄窿邃竇竊竍竏竕竓站竚竝竡竢竦竭竰笂笏笊笆笳笘笙笞笵笨笶筐");
}
#[test]
fn test_sjis_round_trip_row_68() {
    assert_round_trips("筺笄筍笋筌筅筵筥筴筧筰筱筬筮箝箘箟箍箜箚箋箒箏筝箙篋篁篌篏箴篆篝篩簑簔篦篥籠簀簇簓篳篷簗簍篶簣簧簪簟簷簫簽籌籃籔籏籀籐籘籟籤籖籥籬籵粃粐粤粭粢粫粡粨粳粲粱粮粹粽糀糅糂糘糒糜糢鬻糯糲糴糶糺紆");
}
#[test]
fn test_sjis_round_trip_row_69() {
    assert_round_trips("紂紜紕紊絅絋紮紲紿紵絆絳絖絎絲絨絮絏絣經綉絛綏絽綛綺綮綣綵緇綽綫總綢綯緜綸綟綰緘緝緤緞緻緲緡縅縊縣縡縒縱縟縉縋縢繆繦縻縵縹繃縷縲縺繧繝繖繞繙繚繹繪繩繼繻纃緕繽辮繿纈纉續纒纐纓纔纖纎纛纜缸缺");
}
#[test]
fn test_sjis_round_trip_row_70() {
    assert_round_trips("罅罌罍罎罐网罕罔罘罟罠罨罩罧罸羂羆羃羈羇羌羔羞羝羚羣羯羲羹羮羶羸譱翅翆翊翕翔翡翦翩翳翹飜耆耄耋耒耘耙耜耡耨耿耻聊聆聒聘聚聟聢聨聳聲聰聶聹聽聿肄肆肅肛肓肚肭冐肬胛胥胙胝胄胚胖脉胯胱脛脩脣脯腋");
}
#[test]
fn test_sjis_round_trip_row_71() {
    assert_round_trips("隋腆脾腓腑胼腱腮腥腦腴膃膈膊膀膂膠膕膤膣腟膓膩膰膵膾膸膽臀臂膺臉臍臑臙臘臈臚臟臠臧臺臻臾舁舂舅與舊舍舐舖舩舫舸舳艀艙艘艝艚艟艤艢艨艪艫舮艱艷艸艾芍芒芫芟芻芬苡苣苟苒苴苳苺莓范苻苹苞茆苜茉苙");
}
#[test]
fn test_sjis_round_trip_row_72() {
    assert_round_trips("茵茴茖茲茱荀茹荐荅茯茫茗茘莅莚莪莟莢莖茣莎莇莊荼莵荳荵莠莉莨菴萓菫菎菽萃菘萋菁菷萇菠菲萍萢萠莽萸蔆菻葭萪萼蕚蒄葷葫蒭葮蒂葩葆萬葯葹萵蓊葢蒹蒿蒟蓙蓍蒻蓚蓐蓁蓆蓖蒡蔡蓿蓴蔗蔘蔬蔟蔕蔔蓼蕀蕣蕘蕈");
}
#[test]
fn test_sjis_round_trip_row_73() {
    assert_round_trips("蕁蘂蕋蕕薀薤薈薑薊薨蕭薔薛藪薇薜蕷蕾薐藉薺藏薹藐藕藝藥藜藹蘊蘓蘋藾藺蘆蘢蘚蘰蘿虍乕虔號虧虱蚓蚣蚩蚪蚋蚌蚶蚯蛄蛆蚰蛉蠣蚫蛔蛞蛩蛬蛟蛛蛯蜒蜆蜈蜀蜃蛻蜑蜉蜍蛹蜊蜴蜿蜷蜻蜥蜩蜚蝠蝟蝸蝌蝎蝴蝗蝨蝮蝙");
}
#[test]
fn test_sjis_round_trip_row_74() {
    assert_round_trips("蝓蝣蝪蠅螢螟螂螯蟋螽蟀蟐雖螫蟄螳蟇蟆螻蟯蟲蟠蠏蠍蟾蟶蟷蠎蟒蠑蠖蠕蠢蠡蠱蠶蠹蠧蠻衄衂衒衙衞衢衫袁衾袞衵衽袵衲袂袗袒袮袙袢袍袤袰袿袱裃裄裔裘裙裝裹褂裼裴裨裲褄褌褊褓襃褞褥褪褫襁襄褻褶褸襌褝襠襞");
}
#[test]
fn test_sjis_round_trip_row_75() {
    assert_round_trips("襦襤襭襪襯襴襷襾覃覈覊覓覘覡覩覦覬覯覲覺覽覿觀觚觜觝觧觴觸訃訖訐訌訛訝訥訶詁詛詒詆詈詼詭詬詢誅誂誄誨誡誑誥誦誚誣諄諍諂諚諫諳諧諤諱謔諠諢諷諞諛謌謇謚諡謖謐謗謠謳鞫謦謫謾謨譁譌譏譎證譖譛譚譫");
}
#[test]
fn test_sjis_round_trip_row_76() {
    assert_round_trips("譟譬譯譴譽讀讌讎讒讓讖讙讚谺豁谿豈豌豎豐豕豢豬豸豺貂貉貅貊貍貎貔豼貘戝貭貪貽貲貳貮貶賈賁賤賣賚賽賺賻贄贅贊贇贏贍贐齎贓賍贔贖赧赭赱赳趁趙跂趾趺跏跚跖跌跛跋跪跫跟跣跼踈踉跿踝踞踐踟蹂踵踰踴蹊");
}
#[test]
fn test_sjis_round_trip_row_77() {
    assert_round_trips("蹇蹉蹌蹐蹈蹙蹤蹠踪蹣蹕蹶蹲蹼躁躇躅躄躋躊躓躑躔躙躪躡躬躰軆躱躾軅軈軋軛軣軼軻軫軾輊輅輕輒輙輓輜輟輛輌輦輳輻輹轅轂輾轌轉轆轎轗轜轢轣轤辜辟辣辭辯辷迚迥迢迪迯邇迴逅迹迺逑逕逡逍逞逖逋逧逶逵逹迸");
}
#[test]
fn test_sjis_round_trip_row_78() {
    assert_round_trips("遏遐遑遒逎遉逾遖遘遞遨遯遶隨遲邂遽邁邀邊邉邏邨邯邱邵郢郤扈郛鄂鄒鄙鄲鄰酊酖酘酣酥酩酳酲醋醉醂醢醫醯醪醵醴醺釀釁釉釋釐釖釟釡釛釼釵釶鈞釿鈔鈬鈕鈑鉞鉗鉅鉉鉤鉈銕鈿鉋鉐銜銖銓銛鉚鋏銹銷鋩錏鋺鍄錮");
}
#[test]
fn test_sjis_round_trip_row_79() {
    assert_round_trips("錙錢錚錣錺錵錻鍜鍠鍼鍮鍖鎰鎬鎭鎔鎹鏖鏗鏨鏥鏘鏃鏝鏐鏈鏤鐚鐔鐓鐃鐇鐐鐶鐫鐵鐡鐺鑁鑒鑄鑛鑠鑢鑞鑪鈩鑰鑵鑷鑽鑚鑼鑾钁鑿閂閇閊閔閖閘閙閠閨閧閭閼閻閹閾闊濶闃闍闌闕闔闖關闡闥闢阡阨阮阯陂陌陏陋陷陜陞");
}
#[test]
fn test_sjis_round_trip_row_80() {
    assert_round_trips("陝陟陦陲陬隍隘隕隗險隧隱隲隰隴隶隸隹雎雋雉雍襍雜霍雕雹霄霆霈霓霎霑霏霖霙霤霪霰霹霽霾靄靆靈靂靉靜靠靤靦靨勒靫靱靹鞅靼鞁靺鞆鞋鞏鞐鞜鞨鞦鞣鞳鞴韃韆韈韋韜韭齏韲竟韶韵頏頌頸頤頡頷頽顆顏顋顫顯顰");
}
#[test]
fn test_sjis_round_trip_row_81() {
    assert_round_trips("顱顴顳颪颯颱颶飄飃飆飩飫餃餉餒餔餘餡餝餞餤餠餬餮餽餾饂饉饅饐饋饑饒饌饕馗馘馥馭馮馼駟駛駝駘駑駭駮駱駲駻駸騁騏騅駢騙騫騷驅驂驀驃騾驕驍驛驗驟驢驥驤驩驫驪骭骰骼髀髏髑髓體髞髟髢髣髦髯髫髮髴髱髷");
}
#[test]
fn test_sjis_round_trip_row_82() {
    assert_round_trips("髻鬆鬘鬚鬟鬢鬣鬥鬧鬨鬩鬪鬮鬯鬲魄魃魏魍魎魑魘魴鮓鮃鮑鮖鮗鮟鮠鮨鮴鯀鯊鮹鯆鯏鯑鯒鯣鯢鯤鯔鯡鰺鯲鯱鯰鰕鰔鰉鰓鰌鰆鰈鰒鰊鰄鰮鰛鰥鰤鰡鰰鱇鰲鱆鰾鱚鱠鱧鱶鱸鳧鳬鳰鴉鴈鳫鴃鴆鴪鴦鶯鴣鴟鵄鴕鴒鵁鴿鴾鵆鵈");
}
#[test]
fn test_sjis_round_trip_row_83() {
    assert_round_trips("鵝鵞鵤鵑鵐鵙鵲鶉鶇鶫鵯鵺鶚鶤鶩鶲鷄鷁鶻鶸鶺鷆鷏鷂鷙鷓鷸鷦鷭鷯鷽鸚鸛鸞鹵鹹鹽麁麈麋麌麒麕麑麝麥麩麸麪麭靡黌黎黏黐黔黜點黝黠黥黨黯黴黶黷黹黻黼黽鼇鼈皷鼕鼡鼬鼾齊齒齔齣齟齠齡齦齧齬齪齷齲齶龕龜龠");
}
#[test]
fn test_sjis_round_trip_row_84() {
    assert_round_trips("堯槇遙瑤凜熙・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・");
}
#[test]
fn test_sjis_round_trip_row_85() {
    assert_round_trips("・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・");
}
#[test]
fn test_sjis_round_trip_row_86() {
    assert_round_trips("・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・");
}
#[test]
fn test_sjis_round_trip_row_87() {
    assert_round_trips("・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・");
}
#[test]
fn test_sjis_round_trip_row_88() {
    assert_round_trips("・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・");
}
#[test]
fn test_sjis_round_trip_row_89() {
    assert_round_trips("纊褜鍈銈蓜俉炻昱棈鋹曻彅丨仡仼伀伃伹佖侒侊侚侔俍偀倢俿倞偆偰偂傔僴僘兊兤冝冾凬刕劜劦勀勛匀匇匤卲厓厲叝﨎咜咊咩哿喆坙坥垬埈埇﨏塚增墲夋奓奛奝奣妤妺孖寀甯寘寬尞岦岺峵崧嵓﨑嵂嵭嶸嶹巐弡弴彧德");
}
#[test]
fn test_sjis_round_trip_row_90() {
    assert_round_trips("忞恝悅悊惞惕愠惲愑愷愰憘戓抦揵摠撝擎敎昀昕昻昉昮昞昤晥晗晙晴晳暙暠暲暿曺朎朗杦枻桒柀栁桄棏﨓楨﨔榘槢樰橫橆橳橾櫢櫤毖氿汜沆汯泚洄涇浯涖涬淏淸淲淼渹湜渧渼溿澈澵濵瀅瀇瀨炅炫焏焄煜煆煇凞燁燾犱");
}
#[test]
fn test_sjis_round_trip_row_91() {
    assert_round_trips("犾猤猪獷玽珉珖珣珒琇珵琦琪琩琮瑢璉璟甁畯皂皜皞皛皦益睆劯砡硎硤硺礰礼神祥禔福禛竑竧靖竫箞精絈絜綷綠緖繒罇羡羽茁荢荿菇菶葈蒴蕓蕙蕫﨟薰蘒﨡蠇裵訒訷詹誧誾諟諸諶譓譿賰賴贒赶﨣軏﨤逸遧郞都鄕鄧釚");
}
#[test]
fn test_sjis_round_trip_row_92() {
    assert_round_trips("釗釞釭釮釤釥鈆鈐鈊鈺鉀鈼鉎鉙鉑鈹鉧銧鉷鉸鋧鋗鋙鋐﨧鋕鋠鋓錥錡鋻﨨錞鋿錝錂鍰鍗鎤鏆鏞鏸鐱鑅鑈閒隆﨩隝隯霳霻靃靍靏靑靕顗顥飯飼餧館馞驎髙髜魵魲鮏鮱鮻鰀鵰鵫鶴鸙黑・・ⅰⅱⅲⅳⅴⅵⅶⅷⅸⅹ￢￤＇＂");
}
#[test]
fn test_sjis_round_trip_row_93() {
    assert_round_trips("・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・");
}
#[test]
fn test_sjis_round_trip_row_94() {
    assert_round_trips("・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・");
}
#[test]
fn test_sjis_round_trip_row_95() {
    assert_round_trips("\u{E000}\u{E001}\u{E002}\u{E003}\u{E004}\u{E005}\u{E006}\u{E007}\u{E008}\u{E009}\u{E00A}\u{E00B}\u{E00C}\u{E00D}\u{E00E}\u{E00F}\u{E010}\u{E011}\u{E012}\u{E013}\u{E014}\u{E015}\u{E016}\u{E017}\u{E018}\u{E019}\u{E01A}\u{E01B}\u{E01C}\u{E01D}\u{E01E}\u{E01F}\u{E020}\u{E021}\u{E022}\u{E023}\u{E024}\u{E025}\u{E026}\u{E027}\u{E028}\u{E029}\u{E02A}\u{E02B}\u{E02C}\u{E02D}\u{E02E}\u{E02F}\u{E030}\u{E031}\u{E032}\u{E033}\u{E034}\u{E035}\u{E036}\u{E037}\u{E038}\u{E039}\u{E03A}\u{E03B}\u{E03C}\u{E03D}\u{E03E}\u{E03F}\u{E040}\u{E041}\u{E042}\u{E043}\u{E044}\u{E045}\u{E046}\u{E047}\u{E048}\u{E049}\u{E04A}\u{E04B}\u{E04C}\u{E04D}\u{E04E}\u{E04F}\u{E050}\u{E051}\u{E052}\u{E053}\u{E054}\u{E055}\u{E056}\u{E057}\u{E058}\u{E059}\u{E05A}\u{E05B}\u{E05C}\u{E05D}");
}
#[test]
fn test_sjis_round_trip_row_96() {
    assert_round_trips("\u{E05E}\u{E05F}\u{E060}\u{E061}\u{E062}\u{E063}\u{E064}\u{E065}\u{E066}\u{E067}\u{E068}\u{E069}\u{E06A}\u{E06B}\u{E06C}\u{E06D}\u{E06E}\u{E06F}\u{E070}\u{E071}\u{E072}\u{E073}\u{E074}\u{E075}\u{E076}\u{E077}\u{E078}\u{E079}\u{E07A}\u{E07B}\u{E07C}\u{E07D}\u{E07E}\u{E07F}\u{E080}\u{E081}\u{E082}\u{E083}\u{E084}\u{E085}\u{E086}\u{E087}\u{E088}\u{E089}\u{E08A}\u{E08B}\u{E08C}\u{E08D}\u{E08E}\u{E08F}\u{E090}\u{E091}\u{E092}\u{E093}\u{E094}\u{E095}\u{E096}\u{E097}\u{E098}\u{E099}\u{E09A}\u{E09B}\u{E09C}\u{E09D}\u{E09E}\u{E09F}\u{E0A0}\u{E0A1}\u{E0A2}\u{E0A3}\u{E0A4}\u{E0A5}\u{E0A6}\u{E0A7}\u{E0A8}\u{E0A9}\u{E0AA}\u{E0AB}\u{E0AC}\u{E0AD}\u{E0AE}\u{E0AF}\u{E0B0}\u{E0B1}\u{E0B2}\u{E0B3}\u{E0B4}\u{E0B5}\u{E0B6}\u{E0B7}\u{E0B8}\u{E0B9}\u{E0BA}\u{E0BB}");
}

// The game's Shift-JIS decode decodes those rows into a private-use characters
// but, for some reason, the game's encoder does NOT allow encoding them back to Shift-JIS
// we ignore them for now.
#[test]
#[ignore]
fn test_sjis_round_trip_row_97() {
    assert_round_trips("\u{E0BC}\u{E0BD}\u{E0BE}\u{E0BF}\u{E0C0}\u{E0C1}\u{E0C2}\u{E0C3}\u{E0C4}\u{E0C5}\u{E0C6}\u{E0C7}\u{E0C8}\u{E0C9}\u{E0CA}\u{E0CB}\u{E0CC}\u{E0CD}\u{E0CE}\u{E0CF}\u{E0D0}\u{E0D1}\u{E0D2}\u{E0D3}\u{E0D4}\u{E0D5}\u{E0D6}\u{E0D7}\u{E0D8}\u{E0D9}\u{E0DA}\u{E0DB}\u{E0DC}\u{E0DD}\u{E0DE}\u{E0DF}\u{E0E0}\u{E0E1}\u{E0E2}\u{E0E3}\u{E0E4}\u{E0E5}\u{E0E6}\u{E0E7}\u{E0E8}\u{E0E9}\u{E0EA}\u{E0EB}\u{E0EC}\u{E0ED}\u{E0EE}\u{E0EF}\u{E0F0}\u{E0F1}\u{E0F2}\u{E0F3}\u{E0F4}\u{E0F5}\u{E0F6}\u{E0F7}\u{E0F8}\u{E0F9}\u{E0FA}\u{E0FB}\u{E0FC}\u{E0FD}\u{E0FE}\u{E0FF}\u{E100}\u{E101}\u{E102}\u{E103}\u{E104}\u{E105}\u{E106}\u{E107}\u{E108}\u{E109}\u{E10A}\u{E10B}\u{E10C}\u{E10D}\u{E10E}\u{E10F}\u{E110}\u{E111}\u{E112}\u{E113}\u{E114}\u{E115}\u{E116}\u{E117}\u{E118}\u{E119}");
}
#[test]
#[ignore]
fn test_sjis_round_trip_row_98() {
    assert_round_trips("\u{E11A}\u{E11B}\u{E11C}\u{E11D}\u{E11E}\u{E11F}\u{E120}\u{E121}\u{E122}\u{E123}\u{E124}\u{E125}\u{E126}\u{E127}\u{E128}\u{E129}\u{E12A}\u{E12B}\u{E12C}\u{E12D}\u{E12E}\u{E12F}\u{E130}\u{E131}\u{E132}\u{E133}\u{E134}\u{E135}\u{E136}\u{E137}\u{E138}\u{E139}\u{E13A}\u{E13B}\u{E13C}\u{E13D}\u{E13E}\u{E13F}\u{E140}\u{E141}\u{E142}\u{E143}\u{E144}\u{E145}\u{E146}\u{E147}\u{E148}\u{E149}\u{E14A}\u{E14B}\u{E14C}\u{E14D}\u{E14E}\u{E14F}\u{E150}\u{E151}\u{E152}\u{E153}\u{E154}\u{E155}\u{E156}\u{E157}\u{E158}\u{E159}\u{E15A}\u{E15B}\u{E15C}\u{E15D}\u{E15E}\u{E15F}\u{E160}\u{E161}\u{E162}\u{E163}\u{E164}\u{E165}\u{E166}\u{E167}\u{E168}\u{E169}\u{E16A}\u{E16B}\u{E16C}\u{E16D}\u{E16E}\u{E16F}\u{E170}\u{E171}\u{E172}\u{E173}\u{E174}\u{E175}\u{E176}\u{E177}");
}
#[test]
#[ignore]
fn test_sjis_round_trip_row_99() {
    assert_round_trips("\u{E178}\u{E179}\u{E17A}\u{E17B}\u{E17C}\u{E17D}\u{E17E}\u{E17F}\u{E180}\u{E181}\u{E182}\u{E183}\u{E184}\u{E185}\u{E186}\u{E187}\u{E188}\u{E189}\u{E18A}\u{E18B}\u{E18C}\u{E18D}\u{E18E}\u{E18F}\u{E190}\u{E191}\u{E192}\u{E193}\u{E194}\u{E195}\u{E196}\u{E197}\u{E198}\u{E199}\u{E19A}\u{E19B}\u{E19C}\u{E19D}\u{E19E}\u{E19F}\u{E1A0}\u{E1A1}\u{E1A2}\u{E1A3}\u{E1A4}\u{E1A5}\u{E1A6}\u{E1A7}\u{E1A8}\u{E1A9}\u{E1AA}\u{E1AB}\u{E1AC}\u{E1AD}\u{E1AE}\u{E1AF}\u{E1B0}\u{E1B1}\u{E1B2}\u{E1B3}\u{E1B4}\u{E1B5}\u{E1B6}\u{E1B7}\u{E1B8}\u{E1B9}\u{E1BA}\u{E1BB}\u{E1BC}\u{E1BD}\u{E1BE}\u{E1BF}\u{E1C0}\u{E1C1}\u{E1C2}\u{E1C3}\u{E1C4}\u{E1C5}\u{E1C6}\u{E1C7}\u{E1C8}\u{E1C9}\u{E1CA}\u{E1CB}\u{E1CC}\u{E1CD}\u{E1CE}\u{E1CF}\u{E1D0}\u{E1D1}\u{E1D2}\u{E1D3}\u{E1D4}\u{E1D5}");
}
#[test]
#[ignore]
fn test_sjis_round_trip_row_100() {
    assert_round_trips("\u{E1D6}\u{E1D7}\u{E1D8}\u{E1D9}\u{E1DA}\u{E1DB}\u{E1DC}\u{E1DD}\u{E1DE}\u{E1DF}\u{E1E0}\u{E1E1}\u{E1E2}\u{E1E3}\u{E1E4}\u{E1E5}\u{E1E6}\u{E1E7}\u{E1E8}\u{E1E9}\u{E1EA}\u{E1EB}\u{E1EC}\u{E1ED}\u{E1EE}\u{E1EF}\u{E1F0}\u{E1F1}\u{E1F2}\u{E1F3}\u{E1F4}\u{E1F5}\u{E1F6}\u{E1F7}\u{E1F8}\u{E1F9}\u{E1FA}\u{E1FB}\u{E1FC}\u{E1FD}\u{E1FE}\u{E1FF}\u{E200}\u{E201}\u{E202}\u{E203}\u{E204}\u{E205}\u{E206}\u{E207}\u{E208}\u{E209}\u{E20A}\u{E20B}\u{E20C}\u{E20D}\u{E20E}\u{E20F}\u{E210}\u{E211}\u{E212}\u{E213}\u{E214}\u{E215}\u{E216}\u{E217}\u{E218}\u{E219}\u{E21A}\u{E21B}\u{E21C}\u{E21D}\u{E21E}\u{E21F}\u{E220}\u{E221}\u{E222}\u{E223}\u{E224}\u{E225}\u{E226}\u{E227}\u{E228}\u{E229}\u{E22A}\u{E22B}\u{E22C}\u{E22D}\u{E22E}\u{E22F}\u{E230}\u{E231}\u{E232}\u{E233}");
}
#[test]
#[ignore]
fn test_sjis_round_trip_row_101() {
    assert_round_trips("\u{E234}\u{E235}\u{E236}\u{E237}\u{E238}\u{E239}\u{E23A}\u{E23B}\u{E23C}\u{E23D}\u{E23E}\u{E23F}\u{E240}\u{E241}\u{E242}\u{E243}\u{E244}\u{E245}\u{E246}\u{E247}\u{E248}\u{E249}\u{E24A}\u{E24B}\u{E24C}\u{E24D}\u{E24E}\u{E24F}\u{E250}\u{E251}\u{E252}\u{E253}\u{E254}\u{E255}\u{E256}\u{E257}\u{E258}\u{E259}\u{E25A}\u{E25B}\u{E25C}\u{E25D}\u{E25E}\u{E25F}\u{E260}\u{E261}\u{E262}\u{E263}\u{E264}\u{E265}\u{E266}\u{E267}\u{E268}\u{E269}\u{E26A}\u{E26B}\u{E26C}\u{E26D}\u{E26E}\u{E26F}\u{E270}\u{E271}\u{E272}\u{E273}\u{E274}\u{E275}\u{E276}\u{E277}\u{E278}\u{E279}\u{E27A}\u{E27B}\u{E27C}\u{E27D}\u{E27E}\u{E27F}\u{E280}\u{E281}\u{E282}\u{E283}\u{E284}\u{E285}\u{E286}\u{E287}\u{E288}\u{E289}\u{E28A}\u{E28B}\u{E28C}\u{E28D}\u{E28E}\u{E28F}\u{E290}\u{E291}");
}
#[test]
#[ignore]
fn test_sjis_round_trip_row_102() {
    assert_round_trips("\u{E292}\u{E293}\u{E294}\u{E295}\u{E296}\u{E297}\u{E298}\u{E299}\u{E29A}\u{E29B}\u{E29C}\u{E29D}\u{E29E}\u{E29F}\u{E2A0}\u{E2A1}\u{E2A2}\u{E2A3}\u{E2A4}\u{E2A5}\u{E2A6}\u{E2A7}\u{E2A8}\u{E2A9}\u{E2AA}\u{E2AB}\u{E2AC}\u{E2AD}\u{E2AE}\u{E2AF}\u{E2B0}\u{E2B1}\u{E2B2}\u{E2B3}\u{E2B4}\u{E2B5}\u{E2B6}\u{E2B7}\u{E2B8}\u{E2B9}\u{E2BA}\u{E2BB}\u{E2BC}\u{E2BD}\u{E2BE}\u{E2BF}\u{E2C0}\u{E2C1}\u{E2C2}\u{E2C3}\u{E2C4}\u{E2C5}\u{E2C6}\u{E2C7}\u{E2C8}\u{E2C9}\u{E2CA}\u{E2CB}\u{E2CC}\u{E2CD}\u{E2CE}\u{E2CF}\u{E2D0}\u{E2D1}\u{E2D2}\u{E2D3}\u{E2D4}\u{E2D5}\u{E2D6}\u{E2D7}\u{E2D8}\u{E2D9}\u{E2DA}\u{E2DB}\u{E2DC}\u{E2DD}\u{E2DE}\u{E2DF}\u{E2E0}\u{E2E1}\u{E2E2}\u{E2E3}\u{E2E4}\u{E2E5}\u{E2E6}\u{E2E7}\u{E2E8}\u{E2E9}\u{E2EA}\u{E2EB}\u{E2EC}\u{E2ED}\u{E2EE}\u{E2EF}");
}
#[test]
#[ignore]
fn test_sjis_round_trip_row_103() {
    assert_round_trips("\u{E2F0}\u{E2F1}\u{E2F2}\u{E2F3}\u{E2F4}\u{E2F5}\u{E2F6}\u{E2F7}\u{E2F8}\u{E2F9}\u{E2FA}\u{E2FB}\u{E2FC}\u{E2FD}\u{E2FE}\u{E2FF}\u{E300}\u{E301}\u{E302}\u{E303}\u{E304}\u{E305}\u{E306}\u{E307}\u{E308}\u{E309}\u{E30A}\u{E30B}\u{E30C}\u{E30D}\u{E30E}\u{E30F}\u{E310}\u{E311}\u{E312}\u{E313}\u{E314}\u{E315}\u{E316}\u{E317}\u{E318}\u{E319}\u{E31A}\u{E31B}\u{E31C}\u{E31D}\u{E31E}\u{E31F}\u{E320}\u{E321}\u{E322}\u{E323}\u{E324}\u{E325}\u{E326}\u{E327}\u{E328}\u{E329}\u{E32A}\u{E32B}\u{E32C}\u{E32D}\u{E32E}\u{E32F}\u{E330}\u{E331}\u{E332}\u{E333}\u{E334}\u{E335}\u{E336}\u{E337}\u{E338}\u{E339}\u{E33A}\u{E33B}\u{E33C}\u{E33D}\u{E33E}\u{E33F}\u{E340}\u{E341}\u{E342}\u{E343}\u{E344}\u{E345}\u{E346}\u{E347}\u{E348}\u{E349}\u{E34A}\u{E34B}\u{E34C}\u{E34D}");
}
#[test]
#[ignore]
fn test_sjis_round_trip_row_104() {
    assert_round_trips("\u{E34E}\u{E34F}\u{E350}\u{E351}\u{E352}\u{E353}\u{E354}\u{E355}\u{E356}\u{E357}\u{E358}\u{E359}\u{E35A}\u{E35B}\u{E35C}\u{E35D}\u{E35E}\u{E35F}\u{E360}\u{E361}\u{E362}\u{E363}\u{E364}\u{E365}\u{E366}\u{E367}\u{E368}\u{E369}\u{E36A}\u{E36B}\u{E36C}\u{E36D}\u{E36E}\u{E36F}\u{E370}\u{E371}\u{E372}\u{E373}\u{E374}\u{E375}\u{E376}\u{E377}\u{E378}\u{E379}\u{E37A}\u{E37B}\u{E37C}\u{E37D}\u{E37E}\u{E37F}\u{E380}\u{E381}\u{E382}\u{E383}\u{E384}\u{E385}\u{E386}\u{E387}\u{E388}\u{E389}\u{E38A}\u{E38B}\u{E38C}\u{E38D}\u{E38E}\u{E38F}\u{E390}\u{E391}\u{E392}\u{E393}\u{E394}\u{E395}\u{E396}\u{E397}\u{E398}\u{E399}\u{E39A}\u{E39B}\u{E39C}\u{E39D}\u{E39E}\u{E39F}\u{E3A0}\u{E3A1}\u{E3A2}\u{E3A3}\u{E3A4}\u{E3A5}\u{E3A6}\u{E3A7}\u{E3A8}\u{E3A9}\u{E3AA}\u{E3AB}");
}
#[test]
#[ignore]
fn test_sjis_round_trip_row_105() {
    assert_round_trips("\u{E3AC}\u{E3AD}\u{E3AE}\u{E3AF}\u{E3B0}\u{E3B1}\u{E3B2}\u{E3B3}\u{E3B4}\u{E3B5}\u{E3B6}\u{E3B7}\u{E3B8}\u{E3B9}\u{E3BA}\u{E3BB}\u{E3BC}\u{E3BD}\u{E3BE}\u{E3BF}\u{E3C0}\u{E3C1}\u{E3C2}\u{E3C3}\u{E3C4}\u{E3C5}\u{E3C6}\u{E3C7}\u{E3C8}\u{E3C9}\u{E3CA}\u{E3CB}\u{E3CC}\u{E3CD}\u{E3CE}\u{E3CF}\u{E3D0}\u{E3D1}\u{E3D2}\u{E3D3}\u{E3D4}\u{E3D5}\u{E3D6}\u{E3D7}\u{E3D8}\u{E3D9}\u{E3DA}\u{E3DB}\u{E3DC}\u{E3DD}\u{E3DE}\u{E3DF}\u{E3E0}\u{E3E1}\u{E3E2}\u{E3E3}\u{E3E4}\u{E3E5}\u{E3E6}\u{E3E7}\u{E3E8}\u{E3E9}\u{E3EA}\u{E3EB}\u{E3EC}\u{E3ED}\u{E3EE}\u{E3EF}\u{E3F0}\u{E3F1}\u{E3F2}\u{E3F3}\u{E3F4}\u{E3F5}\u{E3F6}\u{E3F7}\u{E3F8}\u{E3F9}\u{E3FA}\u{E3FB}\u{E3FC}\u{E3FD}\u{E3FE}\u{E3FF}\u{E400}\u{E401}\u{E402}\u{E403}\u{E404}\u{E405}\u{E406}\u{E407}\u{E408}\u{E409}");
}
#[test]
#[ignore]
fn test_sjis_round_trip_row_106() {
    assert_round_trips("\u{E40A}\u{E40B}\u{E40C}\u{E40D}\u{E40E}\u{E40F}\u{E410}\u{E411}\u{E412}\u{E413}\u{E414}\u{E415}\u{E416}\u{E417}\u{E418}\u{E419}\u{E41A}\u{E41B}\u{E41C}\u{E41D}\u{E41E}\u{E41F}\u{E420}\u{E421}\u{E422}\u{E423}\u{E424}\u{E425}\u{E426}\u{E427}\u{E428}\u{E429}\u{E42A}\u{E42B}\u{E42C}\u{E42D}\u{E42E}\u{E42F}\u{E430}\u{E431}\u{E432}\u{E433}\u{E434}\u{E435}\u{E436}\u{E437}\u{E438}\u{E439}\u{E43A}\u{E43B}\u{E43C}\u{E43D}\u{E43E}\u{E43F}\u{E440}\u{E441}\u{E442}\u{E443}\u{E444}\u{E445}\u{E446}\u{E447}\u{E448}\u{E449}\u{E44A}\u{E44B}\u{E44C}\u{E44D}\u{E44E}\u{E44F}\u{E450}\u{E451}\u{E452}\u{E453}\u{E454}\u{E455}\u{E456}\u{E457}\u{E458}\u{E459}\u{E45A}\u{E45B}\u{E45C}\u{E45D}\u{E45E}\u{E45F}\u{E460}\u{E461}\u{E462}\u{E463}\u{E464}\u{E465}\u{E466}\u{E467}");
}
#[test]
#[ignore]
fn test_sjis_round_trip_row_107() {
    assert_round_trips("\u{E468}\u{E469}\u{E46A}\u{E46B}\u{E46C}\u{E46D}\u{E46E}\u{E46F}\u{E470}\u{E471}\u{E472}\u{E473}\u{E474}\u{E475}\u{E476}\u{E477}\u{E478}\u{E479}\u{E47A}\u{E47B}\u{E47C}\u{E47D}\u{E47E}\u{E47F}\u{E480}\u{E481}\u{E482}\u{E483}\u{E484}\u{E485}\u{E486}\u{E487}\u{E488}\u{E489}\u{E48A}\u{E48B}\u{E48C}\u{E48D}\u{E48E}\u{E48F}\u{E490}\u{E491}\u{E492}\u{E493}\u{E494}\u{E495}\u{E496}\u{E497}\u{E498}\u{E499}\u{E49A}\u{E49B}\u{E49C}\u{E49D}\u{E49E}\u{E49F}\u{E4A0}\u{E4A1}\u{E4A2}\u{E4A3}\u{E4A4}\u{E4A5}\u{E4A6}\u{E4A7}\u{E4A8}\u{E4A9}\u{E4AA}\u{E4AB}\u{E4AC}\u{E4AD}\u{E4AE}\u{E4AF}\u{E4B0}\u{E4B1}\u{E4B2}\u{E4B3}\u{E4B4}\u{E4B5}\u{E4B6}\u{E4B7}\u{E4B8}\u{E4B9}\u{E4BA}\u{E4BB}\u{E4BC}\u{E4BD}\u{E4BE}\u{E4BF}\u{E4C0}\u{E4C1}\u{E4C2}\u{E4C3}\u{E4C4}\u{E4C5}");
}
#[test]
#[ignore]
fn test_sjis_round_trip_row_108() {
    assert_round_trips("\u{E4C6}\u{E4C7}\u{E4C8}\u{E4C9}\u{E4CA}\u{E4CB}\u{E4CC}\u{E4CD}\u{E4CE}\u{E4CF}\u{E4D0}\u{E4D1}\u{E4D2}\u{E4D3}\u{E4D4}\u{E4D5}\u{E4D6}\u{E4D7}\u{E4D8}\u{E4D9}\u{E4DA}\u{E4DB}\u{E4DC}\u{E4DD}\u{E4DE}\u{E4DF}\u{E4E0}\u{E4E1}\u{E4E2}\u{E4E3}\u{E4E4}\u{E4E5}\u{E4E6}\u{E4E7}\u{E4E8}\u{E4E9}\u{E4EA}\u{E4EB}\u{E4EC}\u{E4ED}\u{E4EE}\u{E4EF}\u{E4F0}\u{E4F1}\u{E4F2}\u{E4F3}\u{E4F4}\u{E4F5}\u{E4F6}\u{E4F7}\u{E4F8}\u{E4F9}\u{E4FA}\u{E4FB}\u{E4FC}\u{E4FD}\u{E4FE}\u{E4FF}\u{E500}\u{E501}\u{E502}\u{E503}\u{E504}\u{E505}\u{E506}\u{E507}\u{E508}\u{E509}\u{E50A}\u{E50B}\u{E50C}\u{E50D}\u{E50E}\u{E50F}\u{E510}\u{E511}\u{E512}\u{E513}\u{E514}\u{E515}\u{E516}\u{E517}\u{E518}\u{E519}\u{E51A}\u{E51B}\u{E51C}\u{E51D}\u{E51E}\u{E51F}\u{E520}\u{E521}\u{E522}\u{E523}");
}
#[test]
#[ignore]
fn test_sjis_round_trip_row_109() {
    assert_round_trips("\u{E524}\u{E525}\u{E526}\u{E527}\u{E528}\u{E529}\u{E52A}\u{E52B}\u{E52C}\u{E52D}\u{E52E}\u{E52F}\u{E530}\u{E531}\u{E532}\u{E533}\u{E534}\u{E535}\u{E536}\u{E537}\u{E538}\u{E539}\u{E53A}\u{E53B}\u{E53C}\u{E53D}\u{E53E}\u{E53F}\u{E540}\u{E541}\u{E542}\u{E543}\u{E544}\u{E545}\u{E546}\u{E547}\u{E548}\u{E549}\u{E54A}\u{E54B}\u{E54C}\u{E54D}\u{E54E}\u{E54F}\u{E550}\u{E551}\u{E552}\u{E553}\u{E554}\u{E555}\u{E556}\u{E557}\u{E558}\u{E559}\u{E55A}\u{E55B}\u{E55C}\u{E55D}\u{E55E}\u{E55F}\u{E560}\u{E561}\u{E562}\u{E563}\u{E564}\u{E565}\u{E566}\u{E567}\u{E568}\u{E569}\u{E56A}\u{E56B}\u{E56C}\u{E56D}\u{E56E}\u{E56F}\u{E570}\u{E571}\u{E572}\u{E573}\u{E574}\u{E575}\u{E576}\u{E577}\u{E578}\u{E579}\u{E57A}\u{E57B}\u{E57C}\u{E57D}\u{E57E}\u{E57F}\u{E580}\u{E581}");
}
#[test]
#[ignore]
fn test_sjis_round_trip_row_110() {
    assert_round_trips("\u{E582}\u{E583}\u{E584}\u{E585}\u{E586}\u{E587}\u{E588}\u{E589}\u{E58A}\u{E58B}\u{E58C}\u{E58D}\u{E58E}\u{E58F}\u{E590}\u{E591}\u{E592}\u{E593}\u{E594}\u{E595}\u{E596}\u{E597}\u{E598}\u{E599}\u{E59A}\u{E59B}\u{E59C}\u{E59D}\u{E59E}\u{E59F}\u{E5A0}\u{E5A1}\u{E5A2}\u{E5A3}\u{E5A4}\u{E5A5}\u{E5A6}\u{E5A7}\u{E5A8}\u{E5A9}\u{E5AA}\u{E5AB}\u{E5AC}\u{E5AD}\u{E5AE}\u{E5AF}\u{E5B0}\u{E5B1}\u{E5B2}\u{E5B3}\u{E5B4}\u{E5B5}\u{E5B6}\u{E5B7}\u{E5B8}\u{E5B9}\u{E5BA}\u{E5BB}\u{E5BC}\u{E5BD}\u{E5BE}\u{E5BF}\u{E5C0}\u{E5C1}\u{E5C2}\u{E5C3}\u{E5C4}\u{E5C5}\u{E5C6}\u{E5C7}\u{E5C8}\u{E5C9}\u{E5CA}\u{E5CB}\u{E5CC}\u{E5CD}\u{E5CE}\u{E5CF}\u{E5D0}\u{E5D1}\u{E5D2}\u{E5D3}\u{E5D4}\u{E5D5}\u{E5D6}\u{E5D7}\u{E5D8}\u{E5D9}\u{E5DA}\u{E5DB}\u{E5DC}\u{E5DD}\u{E5DE}\u{E5DF}");
}
#[test]
#[ignore]
fn test_sjis_round_trip_row_111() {
    assert_round_trips("\u{E5E0}\u{E5E1}\u{E5E2}\u{E5E3}\u{E5E4}\u{E5E5}\u{E5E6}\u{E5E7}\u{E5E8}\u{E5E9}\u{E5EA}\u{E5EB}\u{E5EC}\u{E5ED}\u{E5EE}\u{E5EF}\u{E5F0}\u{E5F1}\u{E5F2}\u{E5F3}\u{E5F4}\u{E5F5}\u{E5F6}\u{E5F7}\u{E5F8}\u{E5F9}\u{E5FA}\u{E5FB}\u{E5FC}\u{E5FD}\u{E5FE}\u{E5FF}\u{E600}\u{E601}\u{E602}\u{E603}\u{E604}\u{E605}\u{E606}\u{E607}\u{E608}\u{E609}\u{E60A}\u{E60B}\u{E60C}\u{E60D}\u{E60E}\u{E60F}\u{E610}\u{E611}\u{E612}\u{E613}\u{E614}\u{E615}\u{E616}\u{E617}\u{E618}\u{E619}\u{E61A}\u{E61B}\u{E61C}\u{E61D}\u{E61E}\u{E61F}\u{E620}\u{E621}\u{E622}\u{E623}\u{E624}\u{E625}\u{E626}\u{E627}\u{E628}\u{E629}\u{E62A}\u{E62B}\u{E62C}\u{E62D}\u{E62E}\u{E62F}\u{E630}\u{E631}\u{E632}\u{E633}\u{E634}\u{E635}\u{E636}\u{E637}\u{E638}\u{E639}\u{E63A}\u{E63B}\u{E63C}\u{E63D}");
}
#[test]
#[ignore]
fn test_sjis_round_trip_row_112() {
    assert_round_trips("\u{E63E}\u{E63F}\u{E640}\u{E641}\u{E642}\u{E643}\u{E644}\u{E645}\u{E646}\u{E647}\u{E648}\u{E649}\u{E64A}\u{E64B}\u{E64C}\u{E64D}\u{E64E}\u{E64F}\u{E650}\u{E651}\u{E652}\u{E653}\u{E654}\u{E655}\u{E656}\u{E657}\u{E658}\u{E659}\u{E65A}\u{E65B}\u{E65C}\u{E65D}\u{E65E}\u{E65F}\u{E660}\u{E661}\u{E662}\u{E663}\u{E664}\u{E665}\u{E666}\u{E667}\u{E668}\u{E669}\u{E66A}\u{E66B}\u{E66C}\u{E66D}\u{E66E}\u{E66F}\u{E670}\u{E671}\u{E672}\u{E673}\u{E674}\u{E675}\u{E676}\u{E677}\u{E678}\u{E679}\u{E67A}\u{E67B}\u{E67C}\u{E67D}\u{E67E}\u{E67F}\u{E680}\u{E681}\u{E682}\u{E683}\u{E684}\u{E685}\u{E686}\u{E687}\u{E688}\u{E689}\u{E68A}\u{E68B}\u{E68C}\u{E68D}\u{E68E}\u{E68F}\u{E690}\u{E691}\u{E692}\u{E693}\u{E694}\u{E695}\u{E696}\u{E697}\u{E698}\u{E699}\u{E69A}\u{E69B}");
}
#[test]
#[ignore]
fn test_sjis_round_trip_row_113() {
    assert_round_trips("\u{E69C}\u{E69D}\u{E69E}\u{E69F}\u{E6A0}\u{E6A1}\u{E6A2}\u{E6A3}\u{E6A4}\u{E6A5}\u{E6A6}\u{E6A7}\u{E6A8}\u{E6A9}\u{E6AA}\u{E6AB}\u{E6AC}\u{E6AD}\u{E6AE}\u{E6AF}\u{E6B0}\u{E6B1}\u{E6B2}\u{E6B3}\u{E6B4}\u{E6B5}\u{E6B6}\u{E6B7}\u{E6B8}\u{E6B9}\u{E6BA}\u{E6BB}\u{E6BC}\u{E6BD}\u{E6BE}\u{E6BF}\u{E6C0}\u{E6C1}\u{E6C2}\u{E6C3}\u{E6C4}\u{E6C5}\u{E6C6}\u{E6C7}\u{E6C8}\u{E6C9}\u{E6CA}\u{E6CB}\u{E6CC}\u{E6CD}\u{E6CE}\u{E6CF}\u{E6D0}\u{E6D1}\u{E6D2}\u{E6D3}\u{E6D4}\u{E6D5}\u{E6D6}\u{E6D7}\u{E6D8}\u{E6D9}\u{E6DA}\u{E6DB}\u{E6DC}\u{E6DD}\u{E6DE}\u{E6DF}\u{E6E0}\u{E6E1}\u{E6E2}\u{E6E3}\u{E6E4}\u{E6E5}\u{E6E6}\u{E6E7}\u{E6E8}\u{E6E9}\u{E6EA}\u{E6EB}\u{E6EC}\u{E6ED}\u{E6EE}\u{E6EF}\u{E6F0}\u{E6F1}\u{E6F2}\u{E6F3}\u{E6F4}\u{E6F5}\u{E6F6}\u{E6F7}\u{E6F8}\u{E6F9}");
}
#[test]
#[ignore]
fn test_sjis_round_trip_row_114() {
    assert_round_trips("\u{E6FA}\u{E6FB}\u{E6FC}\u{E6FD}\u{E6FE}\u{E6FF}\u{E700}\u{E701}\u{E702}\u{E703}\u{E704}\u{E705}\u{E706}\u{E707}\u{E708}\u{E709}\u{E70A}\u{E70B}\u{E70C}\u{E70D}\u{E70E}\u{E70F}\u{E710}\u{E711}\u{E712}\u{E713}\u{E714}\u{E715}\u{E716}\u{E717}\u{E718}\u{E719}\u{E71A}\u{E71B}\u{E71C}\u{E71D}\u{E71E}\u{E71F}\u{E720}\u{E721}\u{E722}\u{E723}\u{E724}\u{E725}\u{E726}\u{E727}\u{E728}\u{E729}\u{E72A}\u{E72B}\u{E72C}\u{E72D}\u{E72E}\u{E72F}\u{E730}\u{E731}\u{E732}\u{E733}\u{E734}\u{E735}\u{E736}\u{E737}\u{E738}\u{E739}\u{E73A}\u{E73B}\u{E73C}\u{E73D}\u{E73E}\u{E73F}\u{E740}\u{E741}\u{E742}\u{E743}\u{E744}\u{E745}\u{E746}\u{E747}\u{E748}\u{E749}\u{E74A}\u{E74B}\u{E74C}\u{E74D}\u{E74E}\u{E74F}\u{E750}\u{E751}\u{E752}\u{E753}\u{E754}\u{E755}\u{E756}\u{E757}");
}

#[test]
fn test_sjis_round_trip_row_115() {
    assert_round_trips("ⅰⅱⅲⅳⅴⅵⅶⅷⅸⅹⅠⅡⅢⅣⅤⅥⅦⅧⅨⅩ￢￤＇＂㈱№℡∵纊褜鍈銈蓜俉炻昱棈鋹曻彅丨仡仼伀伃伹佖侒侊侚侔俍偀倢俿倞偆偰偂傔僴僘兊兤冝冾凬刕劜劦勀勛匀匇匤卲厓厲叝﨎咜咊咩哿喆坙坥垬埈埇﨏塚增墲");
}
#[test]
fn test_sjis_round_trip_row_116() {
    assert_round_trips("夋奓奛奝奣妤妺孖寀甯寘寬尞岦岺峵崧嵓﨑嵂嵭嶸嶹巐弡弴彧德忞恝悅悊惞惕愠惲愑愷愰憘戓抦揵摠撝擎敎昀昕昻昉昮昞昤晥晗晙晴晳暙暠暲暿曺朎朗杦枻桒柀栁桄棏﨓楨﨔榘槢樰橫橆橳橾櫢櫤毖氿汜沆汯泚洄涇浯");
}
#[test]
fn test_sjis_round_trip_row_117() {
    assert_round_trips("涖涬淏淸淲淼渹湜渧渼溿澈澵濵瀅瀇瀨炅炫焏焄煜煆煇凞燁燾犱犾猤猪獷玽珉珖珣珒琇珵琦琪琩琮瑢璉璟甁畯皂皜皞皛皦益睆劯砡硎硤硺礰礼神祥禔福禛竑竧靖竫箞精絈絜綷綠緖繒罇羡羽茁荢荿菇菶葈蒴蕓蕙蕫﨟薰");
}
#[test]
fn test_sjis_round_trip_row_118() {
    assert_round_trips("蘒﨡蠇裵訒訷詹誧誾諟諸諶譓譿賰賴贒赶﨣軏﨤逸遧郞都鄕鄧釚釗釞釭釮釤釥鈆鈐鈊鈺鉀鈼鉎鉙鉑鈹鉧銧鉷鉸鋧鋗鋙鋐﨧鋕鋠鋓錥錡鋻﨨錞鋿錝錂鍰鍗鎤鏆鏞鏸鐱鑅鑈閒隆﨩隝隯霳霻靃靍靏靑靕顗顥飯飼餧館馞驎髙");
}
#[test]
fn test_sjis_round_trip_row_119() {
    assert_round_trips("髜魵魲鮏鮱鮻鰀鵰鵫鶴鸙黑・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・");
}
#[test]
fn test_sjis_round_trip_row_120() {
    assert_round_trips("・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・・");
}
#[test]
fn test_sjis_round_trip_row_121() {
    assert_round_trips("\u{E000}\u{E001}\u{E002}\u{E003}\u{E004}\u{E005}\u{E006}\u{E007}\u{E008}\u{E009}\u{E00A}\u{E00B}\u{E00C}\u{E00D}\u{E00E}\u{E00F}\u{E010}\u{E011}\u{E012}\u{E013}\u{E014}\u{E015}\u{E016}\u{E017}\u{E018}\u{E019}\u{E01A}\u{E01B}\u{E01C}\u{E01D}\u{E01E}\u{E01F}\u{E020}\u{E021}\u{E022}\u{E023}\u{E024}\u{E025}\u{E026}\u{E027}\u{E028}\u{E029}\u{E02A}\u{E02B}\u{E02C}\u{E02D}\u{E02E}\u{E02F}\u{E030}\u{E031}\u{E032}\u{E033}\u{E034}\u{E035}\u{E036}\u{E037}\u{E038}\u{E039}\u{E03A}\u{E03B}\u{E03C}\u{E03D}\u{E03E}\u{E03F}\u{E040}\u{E041}\u{E042}\u{E043}\u{E044}\u{E045}\u{E046}\u{E047}\u{E048}\u{E049}\u{E04A}\u{E04B}\u{E04C}\u{E04D}\u{E04E}\u{E04F}\u{E050}\u{E051}\u{E052}\u{E053}\u{E054}\u{E055}\u{E056}\u{E057}\u{E058}\u{E059}\u{E05A}\u{E05B}\u{E05C}\u{E05D}");
}
#[test]
fn test_sjis_round_trip_row_122() {
    assert_round_trips("\u{E05E}\u{E05F}\u{E060}\u{E061}\u{E062}\u{E063}\u{E064}\u{E065}\u{E066}\u{E067}\u{E068}\u{E069}\u{E06A}\u{E06B}\u{E06C}\u{E06D}\u{E06E}\u{E06F}\u{E070}\u{E071}\u{E072}\u{E073}\u{E074}\u{E075}\u{E076}\u{E077}\u{E078}\u{E079}\u{E07A}\u{E07B}\u{E07C}\u{E07D}\u{E07E}\u{E07F}\u{E080}\u{E081}\u{E082}\u{E083}\u{E084}\u{E085}\u{E086}\u{E087}\u{E088}\u{E089}\u{E08A}\u{E08B}\u{E08C}\u{E08D}\u{E08E}\u{E08F}\u{E090}\u{E091}\u{E092}\u{E093}\u{E094}\u{E095}\u{E096}\u{E097}\u{E098}\u{E099}\u{E09A}\u{E09B}\u{E09C}\u{E09D}\u{E09E}\u{E09F}\u{E0A0}\u{E0A1}\u{E0A2}\u{E0A3}\u{E0A4}\u{E0A5}\u{E0A6}\u{E0A7}\u{E0A8}\u{E0A9}\u{E0AA}\u{E0AB}\u{E0AC}\u{E0AD}\u{E0AE}\u{E0AF}\u{E0B0}\u{E0B1}\u{E0B2}\u{E0B3}\u{E0B4}\u{E0B5}\u{E0B6}\u{E0B7}\u{E0B8}\u{E0B9}\u{E0BA}\u{E0BB}");
}
