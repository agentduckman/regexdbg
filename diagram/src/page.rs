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
          var n=p.replace(/\(\?P</g,'(?<');
          var e=n.replace(/\//g,'\\/');
          RegExper.render('/'+e+'/',el);
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
    s.push_str("<style>body{margin:1rem;font-family:sans-serif}");
    s.push_str("#status{color:#888;font-size:.85rem}</style>");
    s.push_str("</head><body>");
    s.push_str("<div id=\"status\">Connecting\u{2026}</div>");
    s.push_str("<div id=\"d\"></div>");
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
        assert!(h.contains("\\(\\?P<"), "missing PCRE2 normalization regex");
    }
}
