import React, { useEffect } from "react";
import { createRoot } from "react-dom/client";
import {
  scenarios,
  layoutContract,
  handoffContract,
  version,
} from "./blueprint";
import {
  sections,
  vocabulary,
  feasibility,
  backlogAudit,
  managementContract,
} from "./design-contract";
import "./document.css";
const proto = (s) => `/prototype.html?film=off&state=${s}`;
function Doc() {
  useEffect(() => {
    const frame = requestAnimationFrame(() =>
      document.getElementById(location.hash.slice(1))?.scrollIntoView(),
    );
    return () => cancelAnimationFrame(frame);
  }, []);
  return (
    <div className="doc-layout">
      <aside className="doc-nav">
        <a className="doc-brand" href="#overview">
          NuncioCrew<span>Design blueprint / v{version}</span>
        </a>
        <a className="open-prototype" href="/prototype.html">
          Mở prototype ↗
        </a>
        <nav aria-label="Mục lục">
          <a href="#overview">Tổng quan & cách đọc</a>
          <a href="#authority">Trạng thái quyết định</a>
          <a href="#journey">Luồng tổng thể</a>
          <span>Ý NGHĨA TỪNG KHU VỰC</span>
          {sections.map((s) => (
            <a href={`#${s.id}`} key={s.id}>
              <small>{s.number}</small>
              {s.title}
            </a>
          ))}
          <span>HỢP ĐỒNG HÀNH VI</span>
          <a href="#thread-lifecycle">Thread, attention & hoàn tất</a>
          <a href="#management">Add / Edit & thông tin Git</a>
          <a href="#feasibility">Khả năng triển khai backend</a>
          <a href="#states">Các trạng thái & kiểm tra</a>
          <a href="#vocabulary">Thuật ngữ & ranh giới</a>
          <a href="#handoff">Hướng dẫn cho agents</a>
          <a href="#references">Nguồn tham chiếu</a>
        </nav>
        <div className="nav-footer">
          Một tài liệu được cập nhật liên tục.
          <br />
          Prototype ≠ sản phẩm đã triển khai.
        </div>
      </aside>
      <main className="doc-main">
        <header className="doc-top">
          <span>PRODUCT / DESIGN REFERENCE</span>
          <span>08.09.2026 · Bản làm việc</span>
        </header>
        <section id="overview" className="intro">
          <div className="eyebrow">NUNCIOCREW · COMPANYOS</div>
          <h1>Thiết kế không gian làm việc.</h1>
          <p className="lead">
            Ý nghĩa, hành vi và ranh giới của từng phần trong app — để cùng
            duyệt thiết kế và để agents triển khai đúng điều đã thống nhất.
          </p>
          <div className="intro-actions">
            <a className="primary-link" href={proto("thread-working")}>
              Duyệt giao diện tương tác ↗
            </a>
            <a href="#handoff">Đọc trước khi implement ↓</a>
            <a href="#feasibility">Khả năng triển khai backend ↓</a>
          </div>
          <div className="reading-note">
            <strong>Hai trang, hai nhiệm vụ</strong>
            <p>
              <code>index.html</code> giải thích thiết kế.{" "}
              <code>prototype.html</code> cho thử giao diện và các trạng thái.
              Các link “Thử trạng thái” đưa bạn tới đúng ví dụ; dữ liệu trong đó
              hoàn toàn mô phỏng.
            </p>
          </div>
        </section>
        <section id="authority">
          <div className="section-kicker">A / QUYẾT ĐỊNH</div>
          <h2>Đã chọn layout; chưa duyệt mọi hành vi.</h2>
          <p>
            Yêu cầu của founder xác nhận các nhóm sidebar và vùng hội thoại ở
            giữa. Những chi tiết dưới đây làm rõ đề xuất để review; chúng không
            tự trở thành cam kết sản phẩm.
          </p>
          <h3>Stage 0 · accepted / proposed / blocked</h3>
          <p>
            Conceptual Projects/Channels split is accepted. Detailed UI remains
            under discussion; mock controls are not approval or runtime
            evidence.
          </p>
          <div style={{ overflowX: "auto" }}>
            <table>
              <thead>
                <tr>
                  {[
                    "State/control",
                    "Status",
                    "Source",
                    "Implementation contract",
                    "Owner",
                  ].map((h) => (
                    <th key={h}>{h}</th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {handoffContract.map((row) => (
                  <tr key={row.id}>
                    <td>
                      <strong>{row.id}</strong>
                      <br />
                      {row.state}
                    </td>
                    <td>{row.status}</td>
                    <td>{row.source}</td>
                    <td>{row.contract}</td>
                    <td>{row.owner}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <div className="decision-list">
            {layoutContract.map(([id, status, text]) => (
              <div key={id}>
                <span
                  className={`badge ${status.startsWith("Accepted") ? "accepted" : ""}`}
                >
                  {id} ·{" "}
                  {status.startsWith("Accepted") ? "Đã chọn hướng" : "Đề xuất"}
                </span>
                <p>{text}</p>
              </div>
            ))}
          </div>
          <p className="source-note">
            D-078 ghi nhận hướng layout mới và thay phần cấm Projects trên
            sidebar của D-066. Việc bỏ một Workbench picker riêng theo D-065 vẫn
            giữ nguyên. PRODUCT.md / DECISIONS.md sở hữu quyết định sản phẩm;
            tài liệu này sở hữu phần giải thích và tham chiếu tương tác.
          </p>
        </section>
        <section id="journey">
          <div className="section-kicker">B / LUỒNG TỔNG THỂ</div>
          <h2>Từ chọn việc đến kiểm tra kết quả.</h2>
          <ol className="journey-list">
            {[
              [
                "Chọn ngữ cảnh",
                "Bắt đầu từ Inbox, một Project hoặc cuộc trò chuyện riêng với agent.",
                "inbox",
              ],
              [
                "Mở cuộc hội thoại",
                "Channel để nhìn tổng thể; thread để đi sâu vào công việc cụ thể.",
                "channel",
              ],
              [
                "Làm việc trong thread",
                "Đọc hội thoại và activity; mở công cụ liên quan bên cạnh khi cần.",
                "thread-working",
              ],
              [
                "Giải quyết điều đang chờ",
                "Trả lời câu hỏi của agent hoặc phục hồi kết nối; không đánh đồng các trạng thái.",
                "needs-you",
              ],
              [
                "Review kết quả",
                "Kiểm tra bằng chứng và kết quả thực tế. Hoàn thành turn chưa phải founder acceptance.",
                "done",
              ],
            ].map(([t, p, s]) => (
              <li key={s}>
                <div>
                  <h3>{t}</h3>
                  <p>{p}</p>
                </div>
                <a href={proto(s)}>Thử bước này ↗</a>
              </li>
            ))}
          </ol>
        </section>
        <section className="parts-intro">
          <div className="section-kicker">C / GIẢI THÍCH TỪNG MỤC</div>
          <h2>Mỗi khu vực có một vai trò rõ ràng.</h2>
          <p>
            Vị trí các nhóm là hướng đã chọn. Cách nhóm dữ liệu và hành vi chi
            tiết là đề xuất đang duyệt.
          </p>
        </section>
        {sections.map((s) => (
          <section className="area" id={s.id} key={s.id}>
            <div className="area-title">
              <span className="area-number">{s.number}</span>
              <div>
                <span className="eyebrow">{s.zone}</span>
                <h2>{s.title}</h2>
              </div>
              <a href={proto(s.state)}>Thử trong prototype ↗</a>
            </div>
            <p className="purpose">{s.purpose}</p>
            <dl>
              <div>
                <dt>Hiển thị gì</dt>
                <dd>{s.shows}</dd>
              </div>
              <div>
                <dt>Người dùng làm gì</dt>
                <dd>{s.actions}</dd>
              </div>
              <div className="boundary">
                <dt>Không được hiểu sai</dt>
                <dd>{s.boundary}</dd>
              </div>
              <div>
                <dt>Cần chốt thêm</dt>
                <dd>{s.open}</dd>
              </div>
              <div>
                <dt>Điểm tích hợp hiện có</dt>
                <dd>{s.seam}</dd>
              </div>
            </dl>
            <span className="badge">
              Hành vi đề xuất · cần review trước implementation
            </span>
          </section>
        ))}
        <section id="management">
          <div className="section-kicker">
            v0.8 / AGENT LIFECYCLE, RECAP & PLANS
          </div>
          <h2>Thêm người, thêm role và đọc đúng dữ liệu Git.</h2>
          <h3>Channel roles</h3>
          <p>{managementContract.roles}</p>
          <h3>Channel contact point</h3>
          <p>{managementContract.contactPoint}</p>
          <h3>Agents & runtime configuration</h3>
          <p>{managementContract.agents}</p>
          <h3>Delete agent</h3>
          <p>{managementContract.deletion}</p>
          <h3>Recap settings</h3>
          <p>{managementContract.recap}</p>
          <h3>Agent plans & tasks</h3>
          <p>{managementContract.plans}</p>
          <h3>Branch, worktree, +/− và PR</h3>
          <p>{managementContract.checkout}</p>
          <p>
            PR: xanh Open, tím Merged, đỏ Closed (không merge), xám Draft. CI
            hiển thị riêng, nên PR Open vẫn xanh dù CI Failed; Closed vẫn đỏ dù
            checks đã pass. Tham chiếu{" "}
            <a href="https://primer.style/product/components/state-label/">
              GitHub Primer StateLabel
            </a>
            .
          </p>
          <a className="primary-link" href={proto("agents")}>
            Thử Add agent / Edit ↗
          </a>
        </section>
        <section id="feasibility">
          <div className="section-kicker">
            CODE + GITHUB AUDIT / 09.09.2026 · v0.8
          </div>
          <h2>Mỗi hành vi phải có đường triển khai.</h2>
          <p>
            Historical pre-roadmap audit (not rerun in Stage 0): Đối chiếu
            production code tại local HEAD a61590f2 (code giống GitHub main
            8278d2b1; local chỉ thêm docs), prototype v0.8, toàn bộ 5 issue mở
            và danh sách 134 issue đã đóng; đọc sâu 26 issue liên quan cùng
            comments cần thiết. GitHub có 1 PR mở (#347). 126 test tập trung vào
            plans, activity, agents, Inbox, workspace, routes và terminal pass,
            không skip. Đây là code/test audit, chưa chạy live engine hoặc xác
            minh staging/daily relay. Không tạo, sửa hay đóng issue.
          </p>
          {feasibility.map((f) => (
            <article key={f.title}>
              <h3>{f.title}</h3>
              <p>
                <strong>{f.status}</strong>
              </p>
              <dl className="contract-list">
                <div>
                  <dt>Nền tảng / nguồn dữ liệu</dt>
                  <dd>{f.source}</dd>
                </div>
                <div>
                  <dt>Cần triển khai / kiểm tra</dt>
                  <dd>{f.remaining}</dd>
                </div>
                <div>
                  <dt>Giới hạn phải hiển thị đúng</dt>
                  <dd>{f.limit}</dd>
                </div>
              </dl>
            </article>
          ))}
          <h3 id="backlog-audit">
            Consolidate theo outcome, không theo từng control
          </h3>
          <p>
            Đề xuất: rewrite #344, thêm 3 issue mới; giữ các bug/runtime và CI
            riêng. Chưa áp dụng thay đổi trên GitHub.
          </p>
          {backlogAudit.map((item) => (
            <article key={item.title}>
              <h4>
                {item.url ? <a href={item.url}>{item.title}</a> : item.title}
              </h4>
              <p>{item.action}</p>
            </article>
          ))}
          <p>
            Acceptance cho activity: dùng frame thật, kiểm tra hai agents / hai
            threads không lẫn dữ liệu, tool update không nhân đôi, disconnect
            không thành Done, người không có quyền không nhận telemetry. Role
            phải giống nhau trong cùng channel dù owner thread thay đổi.
          </p>
        </section>
        <section id="thread-lifecycle">
          <div className="section-kicker">ĐỀ XUẤT / 09.09.2026</div>
          <h2>Nhìn channel biết việc nào cần mình.</h2>
          <p>
            Channel là nơi tổng hợp các cuộc trao đổi. Context, plan, PRs và
            Actions ở panel phải thuộc thread đang mở, với tên thread luôn hiện
            trên panel. Trở lại channel đóng panel thread; không ngầm gán một
            thread cho cả channel.
          </p>
          <p>
            Mỗi thread card hiện trạng thái công việc, người phụ trách, activity
            nhận trực tiếp, lý do cần bạn và số PR kèm CI. Bộ lọc Needs you gom
            câu hỏi và kết quả chờ review; In progress gồm đang làm, bị chặn,
            dừng và mất kết nối; Done chỉ gồm kết quả đã được chấp nhận.
            Discussion không có task nên không cần ép thành Done. Không cần bạn
            không đồng nghĩa không quan trọng, không xóa hoặc tự archive.
          </p>
          <h3>Workspace film: app tự diễn ra (v0.4)</h3>
          <p>
            Mở prototype.html tự phát câu chuyện dài 2 phút 12 giây. Người dùng
            không cần bấm vào app: con trỏ mô phỏng Oscar mở channel/thread,
            draft được gõ, agents trả lời từng đoạn, tài liệu mở cạnh cuộc trao
            đổi, CI đổi trạng thái và kết quả được bàn giao. Đây là một câu
            chuyện minh họa, không phải telemetry hay người thật đang thao tác.
          </p>
          <p>
            Câu chuyện đi từ #marketing của HeardBack, qua một dependency trong
            #engineering, rồi trở lại duyệt launch pack. Ngoài coding có copy,
            audience research, campaign schedule và channel #customers. Không
            gửi email, đăng bài, merge PR hay chạy agents thật.
          </p>
          <p>
            Thanh film luôn hiện Simulated, caption cảnh, Play/Pause, timeline,
            chapter, Restart và tốc độ 0.5×/1×/2×/4×. Mặc định tự chạy, không tự
            loop. Acceptance trong phim là hành động của Oscar trong kịch bản;
            nó không ghi founder approval cho sản phẩm hoặc triển khai thật.
          </p>
          <p>
            film.js là nguồn timeline. Mọi khung hình được dựng từ thời gian
            tuyệt đối: tua tới/lùi dựng lại cả thread, tin nhắn, draft, PR và
            receipt, không tích lũy timers gửi tin. Explore app dừng phim tại
            snapshot hiện tại để tự xem và thao tác local. Các link scenario bên
            dưới dùng film=off để không bị playback ghi đè.
          </p>
          <p>
            SVG avatars lấy từ bộ Lobe Icons, với nguồn và giấy phép lưu tại
            public/brands. Icon thao tác tiếp tục dùng Lucide SVG. Không vẽ lại
            logo bằng ký tự, không suy ra role từ provider logo.
          </p>
          <a className="primary-link" href="/prototype.html">
            Xem workspace film tự chạy ↗
          </a>
          <h3>Card và vai trò; demo v0.3 đã được thay bằng film</h3>
          <p>
            Thread dùng nền riêng và border để phân nhóm; amber ở cạnh card
            nghĩa là đang cần founder, kể cả chờ review. Status pill giải thích
            loại trạng thái. Badge PR có mã PR + chữ Passed/Running/Failed; bấm
            mở đúng PR trong thread. Màu hoặc animation không phải tín hiệu duy
            nhất.
          </p>
          <p>
            Role được gán ở channel bằng canvas owner ký (D-043), không gán lại
            theo thread. Header channel hiện roster; hàng participant trong
            thread kế thừa các role đó. Thread owner chỉ là người chịu trách
            nhiệm cho công việc. Card đang chạy lấy message/thought/tool từ cùng
            transcript với Activity, không cần thêm lượt agent để viết câu tóm
            tắt. Thought chỉ hiện khi runtime cung cấp. Ví dụ ở prototype vẫn là
            mô phỏng.
          </p>
          <p>
            Demo v0.3 trước đây chạy một chuỗi hữu hạn trên delivery checklist:
            Drafting → Verifying → Ready for review. Card được đưa vào vùng nhìn
            thấy; phase mới có highlight ngắn, Working có nhịp thở nhẹ, CI
            Running có spinner. Demo dừng trước acceptance; Review result dẫn
            vào thread để người dùng Accept. End demo phục hồi fixture
            checklist, reset/navigation dừng timer. Đây không phải agent hoặc
            check thật.
          </p>
          <p>
            Motion chỉ hỗ trợ đọc thay đổi: hover khoảng 150–180ms, card vào
            khoảng 240ms với delay tối đa 100ms, highlight phase 850ms. Reduced
            Motion tắt mọi animation/transition, giữ nguyên chữ, badge và hành
            vi. Không dùng animation để tự tăng phần trăm tiến độ hoặc suy ra
            completion.
          </p>
          <h3>App biết đã xong bằng cách nào?</h3>
          <ol>
            <li>
              Gắn mục tiêu và tiêu chí hoàn tất với công việc trong thread. Một
              thread có thể có nhiều task; hoàn tất một task không đóng mọi việc
              trong cuộc trao đổi.
            </li>
            <li>
              Nhận tín hiệu có cấu trúc từ lifecycle task/job hiện có: đang
              chạy, chờ input, bị chặn, kết quả bàn giao. Turn kết thúc, agent
              offline, im lặng hoặc câu chat “xong rồi” không phải bằng chứng
              đủ.
            </li>
            <li>
              Owner bàn giao kết quả, phiên bản, bằng chứng kiểm tra và giới
              hạn. Khi đủ điều kiện bàn giao, hiện Ready for review, vẫn cần
              founder.
            </li>
            <li>
              Done cần acceptance được lưu với người xác nhận, thời điểm, phạm
              vi và phiên bản kết quả. Prototype cho thử Accept result trên
              checklist không-code; production phải xác minh tiêu chí trước khi
              ghi acceptance.
            </li>
            <li>
              Thay đổi phạm vi/phiên bản tạo việc tiếp nối hoặc Reopen rõ ràng.
              Giữ receipt cũ trong lịch sử; reply xã giao không tự mở lại task.
            </li>
          </ol>
          <p>
            CI, merge và deployment là các điều kiện riêng theo scope. Việc chỉ
            làm tài liệu không bắt buộc có PR. Việc cần release phải có đúng
            environment, artifact/revision và kiểm tra sau deploy. CI xanh của
            SHA cũ không chứng minh SHA mới đạt. Kết nối mất hoặc dữ liệu quá cũ
            phải hiện Unknown/last seen, không tự chuyển Done.
          </p>
          <h3>Nhiều PR và Actions / CI/CD</h3>
          <p>
            Một thread liên kết 0..n PR bằng repository + PR identity, không
            chọn PR đầu tiên làm toàn bộ công việc. Tab PRs cho chọn từng PR;
            tab Actions hiện workflow/run/attempt, revision, từng job, log và
            deployment của PR đang chọn. PR state, review, checks và deployment
            hiển thị độc lập. PR đóng không merge, cancelled/skipped checks hay
            deployment failed không được coi là pass. Workflows ở sidebar là
            catalog routine; Actions trong thread là những lần chạy liên quan
            đến công việc cụ thể.
          </p>
          <p>
            Đây là đề xuất UI, chưa là schema mới. Implementation phải tái sử
            dụng thread root/replies, lifecycle task/job, observer và GitHub
            PR/CI seam hiện có của Buzz/Crew. Cách tổng hợp nhiều task, lưu
            receipt acceptance, chống sự kiện cũ ghi đè, freshness, quyền
            Accept/Reopen và quyền rerun/cancel/deploy còn phải thiết kế và
            duyệt. Không sao chép state React mẫu thành nguồn dữ liệu
            authoritative.
          </p>
          <a className="primary-link" href={proto("channel")}>
            Thử channel nhiều thread ↗
          </a>
        </section>
        <section id="states">
          <div className="section-kicker">D / CÁC TRẠNG THÁI</div>
          <h2>Cùng một UI, nhiều tình huống cần đúng.</h2>
          <p>
            Danh sách này được render trực tiếp từ <code>src/blueprint.js</code>
            , cùng nguồn với scenario picker của prototype. Agents dùng ID để
            bàn giao và kiểm tra; không chép thành một bộ checklist riêng bị
            lệch theo thời gian.
          </p>
          <div className="states-list">
            {scenarios.map((s) => (
              <details key={s.id} id={`state-${s.id}`}>
                <summary>
                  <code>{s.id}</code>
                  <span>{s.label}</span>
                </summary>
                <div className="state-body">
                  <h3>{s.intent}</h3>
                  <div className="state-columns">
                    <div>
                      <h4>Cách thử</h4>
                      <ol>
                        {s.steps.map((t) => (
                          <li key={t}>{t}</li>
                        ))}
                      </ol>
                    </div>
                    <div>
                      <h4>Kết quả mong đợi</h4>
                      <ul>
                        {s.expected.map((t) => (
                          <li key={t}>{t}</li>
                        ))}
                      </ul>
                    </div>
                  </div>
                  <p className="source-note">Buzz seam: {s.seam}</p>
                  <a href={proto(s.id)}>Mở đúng trạng thái ↗</a>
                </div>
              </details>
            ))}
          </div>
        </section>
        <section id="vocabulary">
          <div className="section-kicker">E / THUẬT NGỮ</div>
          <h2>Các khái niệm không được nhập làm một.</h2>
          <div className="table-scroll">
            <table>
              <thead>
                <tr>
                  <th>Khái niệm</th>
                  <th>Ý nghĩa</th>
                  <th>Ranh giới</th>
                </tr>
              </thead>
              <tbody>
                {vocabulary.map(([a, b, c]) => (
                  <tr key={a}>
                    <th>{a}</th>
                    <td>{b}</td>
                    <td>{c}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </section>
        <section id="handoff">
          <div className="section-kicker">F / HƯỚNG DẪN TRIỂN KHAI</div>
          <h2>Agents bắt đầu từ đây.</h2>
          <ol className="handoff-list">
            <li>
              Đọc PRODUCT.md, DECISIONS.md, FORK.md và guidance kiểm thử hiện
              hành của Crew.
            </li>
            <li>
              Chọn mục và state ID bị ảnh hưởng; đọc mục đích, ranh giới và câu
              hỏi còn mở ở trang này.
            </li>
            <li>
              Mở đúng state trong prototype để đối chiếu. Chỉ triển khai hành vi
              đã được founder duyệt hoặc scope đã cho phép; không tự xem mọi đề
              xuất là được chấp nhận.
            </li>
            <li>
              Cập nhật quyết định ở living docs. Sửa giải thích trong{" "}
              <code>src/design-contract.js</code>, state/rules trong{" "}
              <code>src/blueprint.js</code> và UI mẫu tương ứng ngay trong cùng
              thư mục.
            </li>
            <li>
              Ánh xạ vào Buzz seam hiện có. Không đem timer, dữ liệu giả hoặc
              React state của prototype thành nguồn dữ liệu authoritative trong
              app thật.
            </li>
            <li>
              Kiểm tra luồng thật theo state ID. Theo D-077 khi dùng dữ liệu
              staging; mock UI không chứng minh runtime, auth, streaming hoặc độ
              trễ “done”.
            </li>
            <li>
              Bàn giao state ID, thay đổi, bằng chứng và giới hạn. CI xanh và UI
              đẹp không thay founder acceptance.
            </li>
          </ol>
          <div className="reading-note">
            <strong>Giữ một nguồn cho mỗi loại thông tin</strong>
            <p>
              Docs Crew: quyết định sản phẩm và kiến trúc.{" "}
              <code>design-contract.js</code>: ý nghĩa các vùng.{" "}
              <code>blueprint.js</code>: trạng thái và kỳ vọng. Components: giao
              diện tương tác. README: cách chạy và quyền sở hữu. Không tạo thêm
              bản prototype theo từng task.
            </p>
          </div>
        </section>
        <section id="references">
          <div className="section-kicker">G / THAM CHIẾU</div>
          <h2>Thiết kế này bắt đầu từ đâu.</h2>
          <p>
            The original annotated Codex and Crew captures contain private
            workspace context and remain local. They are intentionally excluded
            from this public source handoff.
          </p>
          <p>
            <a href={proto("thread-working")}>Review the mock-only workspace</a>{" "}
            · <a href={proto("channel")}>Review the mock-only channel</a>
          </p>
          <p className="source-note">
            Thay đổi lần này: tách tài liệu thành index.html và giữ giao diện ở
            prototype.html. Hướng layout D-078 không thay đổi; không sửa app
            production.
          </p>
        </section>
        <footer>
          NuncioCrew · Một blueprint dùng chung, cập nhật cùng quyết định.
        </footer>
      </main>
    </div>
  );
}
createRoot(document.getElementById("root")).render(<Doc />);
