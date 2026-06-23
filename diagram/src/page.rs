const POLLING_JS: &str = r#"var last=null;
function update(){
  fetch('/pattern').then(function(r){return r.text();}).then(function(p){
    if(p!==last){
      last=p;
      var el=document.getElementById('d');
      el.innerHTML='';
      document.getElementById('status').textContent=p?'':'(empty pattern)';
      if(p){
        try{
          // Convert PCRE2 named groups (?P<name>) to JS (?<name>).
          var n=p.replace(/\(\?P</g,'(?<');
          // Convert PCRE2 mode modifiers (?flags) / (?-flags) to escaped literals
          // so RegExper renders them as visible text nodes instead of choking on them.
          n=n.replace(/\(\?([-imsxUu]*)\)/g,function(_,f){return '\\(\\?'+f+'\\)';});
          var e=n.replace(/\//g,'\\/');
          var pr=RegExper.render('/'+e+'/',el);
          if(pr&&pr.then){
            pr.catch(function(err){
              el.innerHTML='<p style="color:red">Render error: '+
                (''+err).replace(/</g,'&lt;').replace(/>/g,'&gt;')+'</p>';
            });
          }
        }catch(err){
          el.innerHTML='<p style="color:red">Cannot visualize: '+
            (''+err).replace(/</g,'&lt;').replace(/>/g,'&gt;')+'</p>';
        }
      }
    }
  }).catch(function(){});
}
update();
setInterval(update,500);"#;

pub fn html() -> String {
    let bundle = include_str!("../vendor/regexper.js");
    let mut s = String::with_capacity(bundle.len() + 2048);
    s.push_str("<!DOCTYPE html><html><head><meta charset=\"utf-8\">");
    s.push_str("<title>regexdbg diagram</title>");
    s.push_str("<style>");
    s.push_str("*{box-sizing:border-box}");
    s.push_str("body{margin:0;padding:2rem;font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',sans-serif;background:#f0f2f5;min-height:100vh}");
    s.push_str("#wrap{background:#fff;border:1px solid #d0d4da;border-radius:8px;padding:1.5rem 2rem;box-shadow:0 1px 4px rgba(0,0,0,.08);overflow-x:auto}");
    s.push_str("header{font-size:.8rem;font-weight:600;letter-spacing:.08em;color:#999;text-transform:uppercase;margin-bottom:1rem}");
    s.push_str("#status{color:#999;font-size:.85rem;margin-bottom:.75rem;min-height:1.2em}");
    s.push_str("#d svg{zoom:1.25}");
    s.push_str("</style>");
    s.push_str("</head><body>");
    s.push_str("<div id=\"wrap\">");
    s.push_str("<header>regexdbg</header>");
    s.push_str("<div id=\"status\">Connecting\u{2026}</div>");
    s.push_str("<div id=\"d\"></div>");
    s.push_str("</div>");
    s.push_str("<script>");
    s.push_str(bundle);
    s.push_str("</script><script>");
    s.push_str(POLLING_JS);
    s.push_str("</script></body></html>");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_contains_required_parts() {
        let h = html();
        assert!(h.contains("fetch('/pattern')"), "missing polling fetch");
        assert!(h.contains("RegExper.render"), "missing RegExper call");
        assert!(h.contains("setInterval"), "missing setInterval");
        assert!(h.contains("id=\"d\""), "missing diagram container");
        assert!(h.contains("\\(\\?P<"), "missing PCRE2 named-group normalization");
        assert!(h.contains("\\\\(\\\\?"), "missing mode-modifier escaped-literal conversion");
    }
}
