(function(){const e=document.createElement("link").relList;if(e&&e.supports&&e.supports("modulepreload"))return;for(const r of document.querySelectorAll('link[rel="modulepreload"]'))s(r);new MutationObserver(r=>{for(const l of r)if(l.type==="childList")for(const i of l.addedNodes)i.tagName==="LINK"&&i.rel==="modulepreload"&&s(i)}).observe(document,{childList:!0,subtree:!0});function n(r){const l={};return r.integrity&&(l.integrity=r.integrity),r.referrerPolicy&&(l.referrerPolicy=r.referrerPolicy),r.crossOrigin==="use-credentials"?l.credentials="include":r.crossOrigin==="anonymous"?l.credentials="omit":l.credentials="same-origin",l}function s(r){if(r.ep)return;r.ep=!0;const l=n(r);fetch(r.href,l)}})();var be,T,Lt,Q,it,Et,At,Pt,Ve,He,ze,se={},It=[],on=/acit|ex(?:s|g|n|p|$)|rph|grid|ows|mnc|ntw|ine[ch]|zoo|^ord|itera/i,we=Array.isArray;function F(t,e){for(var n in e)t[n]=e[n];return t}function Ze(t){t&&t.parentNode&&t.parentNode.removeChild(t)}function Mt(t,e,n){var s,r,l,i={};for(l in e)l=="key"?s=e[l]:l=="ref"?r=e[l]:i[l]=e[l];if(arguments.length>2&&(i.children=arguments.length>3?be.call(arguments,2):n),typeof t=="function"&&t.defaultProps!=null)for(l in t.defaultProps)i[l]===void 0&&(i[l]=t.defaultProps[l]);return pe(t,i,s,r,null)}function pe(t,e,n,s,r){var l={type:t,props:e,key:n,ref:s,__k:null,__:null,__b:0,__e:null,__c:null,constructor:void 0,__v:r??++Lt,__i:-1,__u:0};return r==null&&T.vnode!=null&&T.vnode(l),l}function xe(t){return t.children}function he(t,e){this.props=t,this.context=e}function J(t,e){if(e==null)return t.__?J(t.__,t.__i+1):null;for(var n;e<t.__k.length;e++)if((n=t.__k[e])!=null&&n.__e!=null)return n.__e;return typeof t.type=="function"?J(t):null}function Ht(t){var e,n;if((t=t.__)!=null&&t.__c!=null){for(t.__e=t.__c.base=null,e=0;e<t.__k.length;e++)if((n=t.__k[e])!=null&&n.__e!=null){t.__e=t.__c.base=n.__e;break}return Ht(t)}}function at(t){(!t.__d&&(t.__d=!0)&&Q.push(t)&&!_e.__r++||it!=T.debounceRendering)&&((it=T.debounceRendering)||Et)(_e)}function _e(){for(var t,e,n,s,r,l,i,o=1;Q.length;)Q.length>o&&Q.sort(At),t=Q.shift(),o=Q.length,t.__d&&(n=void 0,s=void 0,r=(s=(e=t).__v).__e,l=[],i=[],e.__P&&((n=F({},s)).__v=s.__v+1,T.vnode&&T.vnode(n),Qe(e.__P,n,s,e.__n,e.__P.namespaceURI,32&s.__u?[r]:null,l,r??J(s),!!(32&s.__u),i),n.__v=s.__v,n.__.__k[n.__i]=n,Ut(l,n,i),s.__e=s.__=null,n.__e!=r&&Ht(n)));_e.__r=0}function zt(t,e,n,s,r,l,i,o,a,u,p){var c,d,g,k,m,_,h,f=s&&s.__k||It,$=e.length;for(a=cn(n,e,f,a,$),c=0;c<$;c++)(g=n.__k[c])!=null&&(d=g.__i==-1?se:f[g.__i]||se,g.__i=c,_=Qe(t,g,d,r,l,i,o,a,u,p),k=g.__e,g.ref&&d.ref!=g.ref&&(d.ref&&Ge(d.ref,null,g),p.push(g.ref,g.__c||k,g)),m==null&&k!=null&&(m=k),(h=!!(4&g.__u))||d.__k===g.__k?a=Nt(g,a,t,h):typeof g.type=="function"&&_!==void 0?a=_:k&&(a=k.nextSibling),g.__u&=-7);return n.__e=m,a}function cn(t,e,n,s,r){var l,i,o,a,u,p=n.length,c=p,d=0;for(t.__k=new Array(r),l=0;l<r;l++)(i=e[l])!=null&&typeof i!="boolean"&&typeof i!="function"?(typeof i=="string"||typeof i=="number"||typeof i=="bigint"||i.constructor==String?i=t.__k[l]=pe(null,i,null,null,null):we(i)?i=t.__k[l]=pe(xe,{children:i},null,null,null):i.constructor==null&&i.__b>0?i=t.__k[l]=pe(i.type,i.props,i.key,i.ref?i.ref:null,i.__v):t.__k[l]=i,a=l+d,i.__=t,i.__b=t.__b+1,o=null,(u=i.__i=un(i,n,a,c))!=-1&&(c--,(o=n[u])&&(o.__u|=2)),o==null||o.__v==null?(u==-1&&(r>p?d--:r<p&&d++),typeof i.type!="function"&&(i.__u|=4)):u!=a&&(u==a-1?d--:u==a+1?d++:(u>a?d--:d++,i.__u|=4))):t.__k[l]=null;if(c)for(l=0;l<p;l++)(o=n[l])!=null&&(2&o.__u)==0&&(o.__e==s&&(s=J(o)),Dt(o,o));return s}function Nt(t,e,n,s){var r,l;if(typeof t.type=="function"){for(r=t.__k,l=0;r&&l<r.length;l++)r[l]&&(r[l].__=t,e=Nt(r[l],e,n,s));return e}t.__e!=e&&(s&&(e&&t.type&&!e.parentNode&&(e=J(t)),n.insertBefore(t.__e,e||null)),e=t.__e);do e=e&&e.nextSibling;while(e!=null&&e.nodeType==8);return e}function un(t,e,n,s){var r,l,i,o=t.key,a=t.type,u=e[n],p=u!=null&&(2&u.__u)==0;if(u===null&&o==null||p&&o==u.key&&a==u.type)return n;if(s>(p?1:0)){for(r=n-1,l=n+1;r>=0||l<e.length;)if((u=e[i=r>=0?r--:l++])!=null&&(2&u.__u)==0&&o==u.key&&a==u.type)return i}return-1}function ot(t,e,n){e[0]=="-"?t.setProperty(e,n??""):t[e]=n==null?"":typeof n!="number"||on.test(e)?n:n+"px"}function ce(t,e,n,s,r){var l,i;e:if(e=="style")if(typeof n=="string")t.style.cssText=n;else{if(typeof s=="string"&&(t.style.cssText=s=""),s)for(e in s)n&&e in n||ot(t.style,e,"");if(n)for(e in n)s&&n[e]==s[e]||ot(t.style,e,n[e])}else if(e[0]=="o"&&e[1]=="n")l=e!=(e=e.replace(Pt,"$1")),i=e.toLowerCase(),e=i in t||e=="onFocusOut"||e=="onFocusIn"?i.slice(2):e.slice(2),t.l||(t.l={}),t.l[e+l]=n,n?s?n.u=s.u:(n.u=Ve,t.addEventListener(e,l?ze:He,l)):t.removeEventListener(e,l?ze:He,l);else{if(r=="http://www.w3.org/2000/svg")e=e.replace(/xlink(H|:h)/,"h").replace(/sName$/,"s");else if(e!="width"&&e!="height"&&e!="href"&&e!="list"&&e!="form"&&e!="tabIndex"&&e!="download"&&e!="rowSpan"&&e!="colSpan"&&e!="role"&&e!="popover"&&e in t)try{t[e]=n??"";break e}catch{}typeof n=="function"||(n==null||n===!1&&e[4]!="-"?t.removeAttribute(e):t.setAttribute(e,e=="popover"&&n==1?"":n))}}function ct(t){return function(e){if(this.l){var n=this.l[e.type+t];if(e.t==null)e.t=Ve++;else if(e.t<n.u)return;return n(T.event?T.event(e):e)}}}function Qe(t,e,n,s,r,l,i,o,a,u){var p,c,d,g,k,m,_,h,f,$,x,b,w,I,H,U,O,S=e.type;if(e.constructor!=null)return null;128&n.__u&&(a=!!(32&n.__u),l=[o=e.__e=n.__e]),(p=T.__b)&&p(e);e:if(typeof S=="function")try{if(h=e.props,f="prototype"in S&&S.prototype.render,$=(p=S.contextType)&&s[p.__c],x=p?$?$.props.value:p.__:s,n.__c?_=(c=e.__c=n.__c).__=c.__E:(f?e.__c=c=new S(h,x):(e.__c=c=new he(h,x),c.constructor=S,c.render=hn),$&&$.sub(c),c.state||(c.state={}),c.__n=s,d=c.__d=!0,c.__h=[],c._sb=[]),f&&c.__s==null&&(c.__s=c.state),f&&S.getDerivedStateFromProps!=null&&(c.__s==c.state&&(c.__s=F({},c.__s)),F(c.__s,S.getDerivedStateFromProps(h,c.__s))),g=c.props,k=c.state,c.__v=e,d)f&&S.getDerivedStateFromProps==null&&c.componentWillMount!=null&&c.componentWillMount(),f&&c.componentDidMount!=null&&c.__h.push(c.componentDidMount);else{if(f&&S.getDerivedStateFromProps==null&&h!==g&&c.componentWillReceiveProps!=null&&c.componentWillReceiveProps(h,x),e.__v==n.__v||!c.__e&&c.shouldComponentUpdate!=null&&c.shouldComponentUpdate(h,c.__s,x)===!1){for(e.__v!=n.__v&&(c.props=h,c.state=c.__s,c.__d=!1),e.__e=n.__e,e.__k=n.__k,e.__k.some(function(C){C&&(C.__=e)}),b=0;b<c._sb.length;b++)c.__h.push(c._sb[b]);c._sb=[],c.__h.length&&i.push(c);break e}c.componentWillUpdate!=null&&c.componentWillUpdate(h,c.__s,x),f&&c.componentDidUpdate!=null&&c.__h.push(function(){c.componentDidUpdate(g,k,m)})}if(c.context=x,c.props=h,c.__P=t,c.__e=!1,w=T.__r,I=0,f){for(c.state=c.__s,c.__d=!1,w&&w(e),p=c.render(c.props,c.state,c.context),H=0;H<c._sb.length;H++)c.__h.push(c._sb[H]);c._sb=[]}else do c.__d=!1,w&&w(e),p=c.render(c.props,c.state,c.context),c.state=c.__s;while(c.__d&&++I<25);c.state=c.__s,c.getChildContext!=null&&(s=F(F({},s),c.getChildContext())),f&&!d&&c.getSnapshotBeforeUpdate!=null&&(m=c.getSnapshotBeforeUpdate(g,k)),U=p,p!=null&&p.type===xe&&p.key==null&&(U=Ot(p.props.children)),o=zt(t,we(U)?U:[U],e,n,s,r,l,i,o,a,u),c.base=e.__e,e.__u&=-161,c.__h.length&&i.push(c),_&&(c.__E=c.__=null)}catch(C){if(e.__v=null,a||l!=null)if(C.then){for(e.__u|=a?160:128;o&&o.nodeType==8&&o.nextSibling;)o=o.nextSibling;l[l.indexOf(o)]=null,e.__e=o}else{for(O=l.length;O--;)Ze(l[O]);Ne(e)}else e.__e=n.__e,e.__k=n.__k,C.then||Ne(e);T.__e(C,e,n)}else l==null&&e.__v==n.__v?(e.__k=n.__k,e.__e=n.__e):o=e.__e=pn(n.__e,e,n,s,r,l,i,a,u);return(p=T.diffed)&&p(e),128&e.__u?void 0:o}function Ne(t){t&&t.__c&&(t.__c.__e=!0),t&&t.__k&&t.__k.forEach(Ne)}function Ut(t,e,n){for(var s=0;s<n.length;s++)Ge(n[s],n[++s],n[++s]);T.__c&&T.__c(e,t),t.some(function(r){try{t=r.__h,r.__h=[],t.some(function(l){l.call(r)})}catch(l){T.__e(l,r.__v)}})}function Ot(t){return typeof t!="object"||t==null||t.__b&&t.__b>0?t:we(t)?t.map(Ot):F({},t)}function pn(t,e,n,s,r,l,i,o,a){var u,p,c,d,g,k,m,_=n.props||se,h=e.props,f=e.type;if(f=="svg"?r="http://www.w3.org/2000/svg":f=="math"?r="http://www.w3.org/1998/Math/MathML":r||(r="http://www.w3.org/1999/xhtml"),l!=null){for(u=0;u<l.length;u++)if((g=l[u])&&"setAttribute"in g==!!f&&(f?g.localName==f:g.nodeType==3)){t=g,l[u]=null;break}}if(t==null){if(f==null)return document.createTextNode(h);t=document.createElementNS(r,f,h.is&&h),o&&(T.__m&&T.__m(e,l),o=!1),l=null}if(f==null)_===h||o&&t.data==h||(t.data=h);else{if(l=l&&be.call(t.childNodes),!o&&l!=null)for(_={},u=0;u<t.attributes.length;u++)_[(g=t.attributes[u]).name]=g.value;for(u in _)if(g=_[u],u!="children"){if(u=="dangerouslySetInnerHTML")c=g;else if(!(u in h)){if(u=="value"&&"defaultValue"in h||u=="checked"&&"defaultChecked"in h)continue;ce(t,u,null,g,r)}}for(u in h)g=h[u],u=="children"?d=g:u=="dangerouslySetInnerHTML"?p=g:u=="value"?k=g:u=="checked"?m=g:o&&typeof g!="function"||_[u]===g||ce(t,u,g,_[u],r);if(p)o||c&&(p.__html==c.__html||p.__html==t.innerHTML)||(t.innerHTML=p.__html),e.__k=[];else if(c&&(t.innerHTML=""),zt(e.type=="template"?t.content:t,we(d)?d:[d],e,n,s,f=="foreignObject"?"http://www.w3.org/1999/xhtml":r,l,i,l?l[0]:n.__k&&J(n,0),o,a),l!=null)for(u=l.length;u--;)Ze(l[u]);o||(u="value",f=="progress"&&k==null?t.removeAttribute("value"):k!=null&&(k!==t[u]||f=="progress"&&!k||f=="option"&&k!=_[u])&&ce(t,u,k,_[u],r),u="checked",m!=null&&m!=t[u]&&ce(t,u,m,_[u],r))}return t}function Ge(t,e,n){try{if(typeof t=="function"){var s=typeof t.__u=="function";s&&t.__u(),s&&e==null||(t.__u=t(e))}else t.current=e}catch(r){T.__e(r,n)}}function Dt(t,e,n){var s,r;if(T.unmount&&T.unmount(t),(s=t.ref)&&(s.current&&s.current!=t.__e||Ge(s,null,e)),(s=t.__c)!=null){if(s.componentWillUnmount)try{s.componentWillUnmount()}catch(l){T.__e(l,e)}s.base=s.__P=null}if(s=t.__k)for(r=0;r<s.length;r++)s[r]&&Dt(s[r],e,n||typeof t.type!="function");n||Ze(t.__e),t.__c=t.__=t.__e=void 0}function hn(t,e,n){return this.constructor(t,n)}function dn(t,e,n){var s,r,l,i;e==document&&(e=document.documentElement),T.__&&T.__(t,e),r=(s=!1)?null:e.__k,l=[],i=[],Qe(e,t=e.__k=Mt(xe,null,[t]),r||se,se,e.namespaceURI,r?null:e.firstChild?be.call(e.childNodes):null,l,r?r.__e:e.firstChild,s,i),Ut(l,t,i)}be=It.slice,T={__e:function(t,e,n,s){for(var r,l,i;e=e.__;)if((r=e.__c)&&!r.__)try{if((l=r.constructor)&&l.getDerivedStateFromError!=null&&(r.setState(l.getDerivedStateFromError(t)),i=r.__d),r.componentDidCatch!=null&&(r.componentDidCatch(t,s||{}),i=r.__d),i)return r.__E=r}catch(o){t=o}throw t}},Lt=0,he.prototype.setState=function(t,e){var n;n=this.__s!=null&&this.__s!=this.state?this.__s:this.__s=F({},this.state),typeof t=="function"&&(t=t(F({},n),this.props)),t&&F(n,t),t!=null&&this.__v&&(e&&this._sb.push(e),at(this))},he.prototype.forceUpdate=function(t){this.__v&&(this.__e=!0,t&&this.__h.push(t),at(this))},he.prototype.render=xe,Q=[],Et=typeof Promise=="function"?Promise.prototype.then.bind(Promise.resolve()):setTimeout,At=function(t,e){return t.__v.__b-e.__v.__b},_e.__r=0,Pt=/(PointerCapture)$|Capture$/i,Ve=0,He=ct(!1),ze=ct(!0);var le,A,Te,ut,ie=0,jt=[],P=T,pt=P.__b,ht=P.__r,dt=P.diffed,ft=P.__c,gt=P.unmount,_t=P.__;function Ke(t,e){P.__h&&P.__h(A,t,ie||e),ie=0;var n=A.__H||(A.__H={__:[],__h:[]});return t>=n.__.length&&n.__.push({}),n.__[t]}function M(t){return ie=1,fn(Bt,t)}function fn(t,e,n){var s=Ke(le++,2);if(s.t=t,!s.__c&&(s.__=[Bt(void 0,e),function(o){var a=s.__N?s.__N[0]:s.__[0],u=s.t(a,o);a!==u&&(s.__N=[u,s.__[1]],s.__c.setState({}))}],s.__c=A,!A.__f)){var r=function(o,a,u){if(!s.__c.__H)return!0;var p=s.__c.__H.__.filter(function(d){return!!d.__c});if(p.every(function(d){return!d.__N}))return!l||l.call(this,o,a,u);var c=s.__c.props!==o;return p.forEach(function(d){if(d.__N){var g=d.__[0];d.__=d.__N,d.__N=void 0,g!==d.__[0]&&(c=!0)}}),l&&l.call(this,o,a,u)||c};A.__f=!0;var l=A.shouldComponentUpdate,i=A.componentWillUpdate;A.componentWillUpdate=function(o,a,u){if(this.__e){var p=l;l=void 0,r(o,a,u),l=p}i&&i.call(this,o,a,u)},A.shouldComponentUpdate=r}return s.__N||s.__}function L(t,e){var n=Ke(le++,3);!P.__s&&qt(n.__H,e)&&(n.__=t,n.u=e,A.__H.__h.push(n))}function V(t){return ie=5,q(function(){return{current:t}},[])}function q(t,e){var n=Ke(le++,7);return qt(n.__H,e)&&(n.__=t(),n.__H=e,n.__h=t),n.__}function N(t,e){return ie=8,q(function(){return t},e)}function gn(){for(var t;t=jt.shift();)if(t.__P&&t.__H)try{t.__H.__h.forEach(de),t.__H.__h.forEach(Ue),t.__H.__h=[]}catch(e){t.__H.__h=[],P.__e(e,t.__v)}}P.__b=function(t){A=null,pt&&pt(t)},P.__=function(t,e){t&&e.__k&&e.__k.__m&&(t.__m=e.__k.__m),_t&&_t(t,e)},P.__r=function(t){ht&&ht(t),le=0;var e=(A=t.__c).__H;e&&(Te===A?(e.__h=[],A.__h=[],e.__.forEach(function(n){n.__N&&(n.__=n.__N),n.u=n.__N=void 0})):(e.__h.forEach(de),e.__h.forEach(Ue),e.__h=[],le=0)),Te=A},P.diffed=function(t){dt&&dt(t);var e=t.__c;e&&e.__H&&(e.__H.__h.length&&(jt.push(e)!==1&&ut===P.requestAnimationFrame||((ut=P.requestAnimationFrame)||_n)(gn)),e.__H.__.forEach(function(n){n.u&&(n.__H=n.u),n.u=void 0})),Te=A=null},P.__c=function(t,e){e.some(function(n){try{n.__h.forEach(de),n.__h=n.__h.filter(function(s){return!s.__||Ue(s)})}catch(s){e.some(function(r){r.__h&&(r.__h=[])}),e=[],P.__e(s,n.__v)}}),ft&&ft(t,e)},P.unmount=function(t){gt&&gt(t);var e,n=t.__c;n&&n.__H&&(n.__H.__.forEach(function(s){try{de(s)}catch(r){e=r}}),n.__H=void 0,e&&P.__e(e,n.__v))};var mt=typeof requestAnimationFrame=="function";function _n(t){var e,n=function(){clearTimeout(s),mt&&cancelAnimationFrame(e),setTimeout(t)},s=setTimeout(n,35);mt&&(e=requestAnimationFrame(n))}function de(t){var e=A,n=t.__c;typeof n=="function"&&(t.__c=void 0,n()),A=e}function Ue(t){var e=A;t.__c=t.__(),A=e}function qt(t,e){return!t||t.length!==e.length||e.some(function(n,s){return n!==t[s]})}function Bt(t,e){return typeof e=="function"?e(t):e}var Ft=function(t,e,n,s){var r;e[0]=0;for(var l=1;l<e.length;l++){var i=e[l++],o=e[l]?(e[0]|=i?1:2,n[e[l++]]):e[++l];i===3?s[0]=o:i===4?s[1]=Object.assign(s[1]||{},o):i===5?(s[1]=s[1]||{})[e[++l]]=o:i===6?s[1][e[++l]]+=o+"":i?(r=t.apply(o,Ft(t,o,n,["",null])),s.push(r),o[0]?e[0]|=2:(e[l-2]=0,e[l]=r)):s.push(o)}return s},kt=new Map;function mn(t){var e=kt.get(this);return e||(e=new Map,kt.set(this,e)),(e=Ft(this,e.get(t)||(e.set(t,e=(function(n){for(var s,r,l=1,i="",o="",a=[0],u=function(d){l===1&&(d||(i=i.replace(/^\s*\n\s*|\s*\n\s*$/g,"")))?a.push(0,d,i):l===3&&(d||i)?(a.push(3,d,i),l=2):l===2&&i==="..."&&d?a.push(4,d,0):l===2&&i&&!d?a.push(5,0,!0,i):l>=5&&((i||!d&&l===5)&&(a.push(l,0,i,r),l=6),d&&(a.push(l,d,0,r),l=6)),i=""},p=0;p<n.length;p++){p&&(l===1&&u(),u(p));for(var c=0;c<n[p].length;c++)s=n[p][c],l===1?s==="<"?(u(),a=[a],l=3):i+=s:l===4?i==="--"&&s===">"?(l=1,i=""):i=s+i[0]:o?s===o?o="":i+=s:s==='"'||s==="'"?o=s:s===">"?(u(),l=1):l&&(s==="="?(l=5,r=i,i=""):s==="/"&&(l<5||n[p][c+1]===">")?(u(),l===3&&(a=a[0]),l=a,(a=a[0]).push(2,0,l),l=0):s===" "||s==="	"||s===`
`||s==="\r"?(u(),l=2):i+=s),l===3&&i==="!--"&&(l=4,a=a[0])}return u(),a})(t)),e),arguments,[])).length>1?e:e[0]}function Xe(){return{async:!1,breaks:!1,extensions:null,gfm:!0,hooks:null,pedantic:!1,renderer:null,silent:!1,tokenizer:null,walkTokens:null}}var K=Xe();function Wt(t){K=t}var re={exec:()=>null};function y(t,e=""){let n=typeof t=="string"?t:t.source,s={replace:(r,l)=>{let i=typeof l=="string"?l:l.source;return i=i.replace(z.caret,"$1"),n=n.replace(r,i),s},getRegex:()=>new RegExp(n,e)};return s}var kn=(()=>{try{return!!new RegExp("(?<=1)(?<!1)")}catch{return!1}})(),z={codeRemoveIndent:/^(?: {1,4}| {0,3}\t)/gm,outputLinkReplace:/\\([\[\]])/g,indentCodeCompensation:/^(\s+)(?:```)/,beginningSpace:/^\s+/,endingHash:/#$/,startingSpaceChar:/^ /,endingSpaceChar:/ $/,nonSpaceChar:/[^ ]/,newLineCharGlobal:/\n/g,tabCharGlobal:/\t/g,multipleSpaceGlobal:/\s+/g,blankLine:/^[ \t]*$/,doubleBlankLine:/\n[ \t]*\n[ \t]*$/,blockquoteStart:/^ {0,3}>/,blockquoteSetextReplace:/\n {0,3}((?:=+|-+) *)(?=\n|$)/g,blockquoteSetextReplace2:/^ {0,3}>[ \t]?/gm,listReplaceTabs:/^\t+/,listReplaceNesting:/^ {1,4}(?=( {4})*[^ ])/g,listIsTask:/^\[[ xX]\] +\S/,listReplaceTask:/^\[[ xX]\] +/,listTaskCheckbox:/\[[ xX]\]/,anyLine:/\n.*\n/,hrefBrackets:/^<(.*)>$/,tableDelimiter:/[:|]/,tableAlignChars:/^\||\| *$/g,tableRowBlankLine:/\n[ \t]*$/,tableAlignRight:/^ *-+: *$/,tableAlignCenter:/^ *:-+: *$/,tableAlignLeft:/^ *:-+ *$/,startATag:/^<a /i,endATag:/^<\/a>/i,startPreScriptTag:/^<(pre|code|kbd|script)(\s|>)/i,endPreScriptTag:/^<\/(pre|code|kbd|script)(\s|>)/i,startAngleBracket:/^</,endAngleBracket:/>$/,pedanticHrefTitle:/^([^'"]*[^\s])\s+(['"])(.*)\2/,unicodeAlphaNumeric:/[\p{L}\p{N}]/u,escapeTest:/[&<>"']/,escapeReplace:/[&<>"']/g,escapeTestNoEncode:/[<>"']|&(?!(#\d{1,7}|#[Xx][a-fA-F0-9]{1,6}|\w+);)/,escapeReplaceNoEncode:/[<>"']|&(?!(#\d{1,7}|#[Xx][a-fA-F0-9]{1,6}|\w+);)/g,unescapeTest:/&(#(?:\d+)|(?:#x[0-9A-Fa-f]+)|(?:\w+));?/ig,caret:/(^|[^\[])\^/g,percentDecode:/%25/g,findPipe:/\|/g,splitPipe:/ \|/,slashPipe:/\\\|/g,carriageReturn:/\r\n|\r/g,spaceLine:/^ +$/gm,notSpaceStart:/^\S*/,endingNewline:/\n$/,listItemRegex:t=>new RegExp(`^( {0,3}${t})((?:[	 ][^\\n]*)?(?:\\n|$))`),nextBulletRegex:t=>new RegExp(`^ {0,${Math.min(3,t-1)}}(?:[*+-]|\\d{1,9}[.)])((?:[ 	][^\\n]*)?(?:\\n|$))`),hrRegex:t=>new RegExp(`^ {0,${Math.min(3,t-1)}}((?:- *){3,}|(?:_ *){3,}|(?:\\* *){3,})(?:\\n+|$)`),fencesBeginRegex:t=>new RegExp(`^ {0,${Math.min(3,t-1)}}(?:\`\`\`|~~~)`),headingBeginRegex:t=>new RegExp(`^ {0,${Math.min(3,t-1)}}#`),htmlBeginRegex:t=>new RegExp(`^ {0,${Math.min(3,t-1)}}<(?:[a-z].*>|!--)`,"i")},vn=/^(?:[ \t]*(?:\n|$))+/,$n=/^((?: {4}| {0,3}\t)[^\n]+(?:\n(?:[ \t]*(?:\n|$))*)?)+/,bn=/^ {0,3}(`{3,}(?=[^`\n]*(?:\n|$))|~{3,})([^\n]*)(?:\n|$)(?:|([\s\S]*?)(?:\n|$))(?: {0,3}\1[~`]* *(?=\n|$)|$)/,oe=/^ {0,3}((?:-[\t ]*){3,}|(?:_[ \t]*){3,}|(?:\*[ \t]*){3,})(?:\n+|$)/,wn=/^ {0,3}(#{1,6})(?=\s|$)(.*)(?:\n+|$)/,Je=/(?:[*+-]|\d{1,9}[.)])/,Vt=/^(?!bull |blockCode|fences|blockquote|heading|html|table)((?:.|\n(?!\s*?\n|bull |blockCode|fences|blockquote|heading|html|table))+?)\n {0,3}(=+|-+) *(?:\n+|$)/,Zt=y(Vt).replace(/bull/g,Je).replace(/blockCode/g,/(?: {4}| {0,3}\t)/).replace(/fences/g,/ {0,3}(?:`{3,}|~{3,})/).replace(/blockquote/g,/ {0,3}>/).replace(/heading/g,/ {0,3}#{1,6}/).replace(/html/g,/ {0,3}<[^\n>]+>\n/).replace(/\|table/g,"").getRegex(),xn=y(Vt).replace(/bull/g,Je).replace(/blockCode/g,/(?: {4}| {0,3}\t)/).replace(/fences/g,/ {0,3}(?:`{3,}|~{3,})/).replace(/blockquote/g,/ {0,3}>/).replace(/heading/g,/ {0,3}#{1,6}/).replace(/html/g,/ {0,3}<[^\n>]+>\n/).replace(/table/g,/ {0,3}\|?(?:[:\- ]*\|)+[\:\- ]*\n/).getRegex(),Ye=/^([^\n]+(?:\n(?!hr|heading|lheading|blockquote|fences|list|html|table| +\n)[^\n]+)*)/,yn=/^[^\n]+/,et=/(?!\s*\])(?:\\[\s\S]|[^\[\]\\])+/,Sn=y(/^ {0,3}\[(label)\]: *(?:\n[ \t]*)?([^<\s][^\s]*|<.*?>)(?:(?: +(?:\n[ \t]*)?| *\n[ \t]*)(title))? *(?:\n+|$)/).replace("label",et).replace("title",/(?:"(?:\\"?|[^"\\])*"|'[^'\n]*(?:\n[^'\n]+)*\n?'|\([^()]*\))/).getRegex(),Rn=y(/^( {0,3}bull)([ \t][^\n]+?)?(?:\n|$)/).replace(/bull/g,Je).getRegex(),ye="address|article|aside|base|basefont|blockquote|body|caption|center|col|colgroup|dd|details|dialog|dir|div|dl|dt|fieldset|figcaption|figure|footer|form|frame|frameset|h[1-6]|head|header|hr|html|iframe|legend|li|link|main|menu|menuitem|meta|nav|noframes|ol|optgroup|option|p|param|search|section|summary|table|tbody|td|tfoot|th|thead|title|tr|track|ul",tt=/<!--(?:-?>|[\s\S]*?(?:-->|$))/,Tn=y("^ {0,3}(?:<(script|pre|style|textarea)[\\s>][\\s\\S]*?(?:</\\1>[^\\n]*\\n+|$)|comment[^\\n]*(\\n+|$)|<\\?[\\s\\S]*?(?:\\?>\\n*|$)|<![A-Z][\\s\\S]*?(?:>\\n*|$)|<!\\[CDATA\\[[\\s\\S]*?(?:\\]\\]>\\n*|$)|</?(tag)(?: +|\\n|/?>)[\\s\\S]*?(?:(?:\\n[ 	]*)+\\n|$)|<(?!script|pre|style|textarea)([a-z][\\w-]*)(?:attribute)*? */?>(?=[ \\t]*(?:\\n|$))[\\s\\S]*?(?:(?:\\n[ 	]*)+\\n|$)|</(?!script|pre|style|textarea)[a-z][\\w-]*\\s*>(?=[ \\t]*(?:\\n|$))[\\s\\S]*?(?:(?:\\n[ 	]*)+\\n|$))","i").replace("comment",tt).replace("tag",ye).replace("attribute",/ +[a-zA-Z:_][\w.:-]*(?: *= *"[^"\n]*"| *= *'[^'\n]*'| *= *[^\s"'=<>`]+)?/).getRegex(),Qt=y(Ye).replace("hr",oe).replace("heading"," {0,3}#{1,6}(?:\\s|$)").replace("|lheading","").replace("|table","").replace("blockquote"," {0,3}>").replace("fences"," {0,3}(?:`{3,}(?=[^`\\n]*\\n)|~{3,})[^\\n]*\\n").replace("list"," {0,3}(?:[*+-]|1[.)]) ").replace("html","</?(?:tag)(?: +|\\n|/?>)|<(?:script|pre|style|textarea|!--)").replace("tag",ye).getRegex(),Cn=y(/^( {0,3}> ?(paragraph|[^\n]*)(?:\n|$))+/).replace("paragraph",Qt).getRegex(),nt={blockquote:Cn,code:$n,def:Sn,fences:bn,heading:wn,hr:oe,html:Tn,lheading:Zt,list:Rn,newline:vn,paragraph:Qt,table:re,text:yn},vt=y("^ *([^\\n ].*)\\n {0,3}((?:\\| *)?:?-+:? *(?:\\| *:?-+:? *)*(?:\\| *)?)(?:\\n((?:(?! *\\n|hr|heading|blockquote|code|fences|list|html).*(?:\\n|$))*)\\n*|$)").replace("hr",oe).replace("heading"," {0,3}#{1,6}(?:\\s|$)").replace("blockquote"," {0,3}>").replace("code","(?: {4}| {0,3}	)[^\\n]").replace("fences"," {0,3}(?:`{3,}(?=[^`\\n]*\\n)|~{3,})[^\\n]*\\n").replace("list"," {0,3}(?:[*+-]|1[.)]) ").replace("html","</?(?:tag)(?: +|\\n|/?>)|<(?:script|pre|style|textarea|!--)").replace("tag",ye).getRegex(),Ln={...nt,lheading:xn,table:vt,paragraph:y(Ye).replace("hr",oe).replace("heading"," {0,3}#{1,6}(?:\\s|$)").replace("|lheading","").replace("table",vt).replace("blockquote"," {0,3}>").replace("fences"," {0,3}(?:`{3,}(?=[^`\\n]*\\n)|~{3,})[^\\n]*\\n").replace("list"," {0,3}(?:[*+-]|1[.)]) ").replace("html","</?(?:tag)(?: +|\\n|/?>)|<(?:script|pre|style|textarea|!--)").replace("tag",ye).getRegex()},En={...nt,html:y(`^ *(?:comment *(?:\\n|\\s*$)|<(tag)[\\s\\S]+?</\\1> *(?:\\n{2,}|\\s*$)|<tag(?:"[^"]*"|'[^']*'|\\s[^'"/>\\s]*)*?/?> *(?:\\n{2,}|\\s*$))`).replace("comment",tt).replace(/tag/g,"(?!(?:a|em|strong|small|s|cite|q|dfn|abbr|data|time|code|var|samp|kbd|sub|sup|i|b|u|mark|ruby|rt|rp|bdi|bdo|span|br|wbr|ins|del|img)\\b)\\w+(?!:|[^\\w\\s@]*@)\\b").getRegex(),def:/^ *\[([^\]]+)\]: *<?([^\s>]+)>?(?: +(["(][^\n]+[")]))? *(?:\n+|$)/,heading:/^(#{1,6})(.*)(?:\n+|$)/,fences:re,lheading:/^(.+?)\n {0,3}(=+|-+) *(?:\n+|$)/,paragraph:y(Ye).replace("hr",oe).replace("heading",` *#{1,6} *[^
]`).replace("lheading",Zt).replace("|table","").replace("blockquote"," {0,3}>").replace("|fences","").replace("|list","").replace("|html","").replace("|tag","").getRegex()},An=/^\\([!"#$%&'()*+,\-./:;<=>?@\[\]\\^_`{|}~])/,Pn=/^(`+)([^`]|[^`][\s\S]*?[^`])\1(?!`)/,Gt=/^( {2,}|\\)\n(?!\s*$)/,In=/^(`+|[^`])(?:(?= {2,}\n)|[\s\S]*?(?:(?=[\\<!\[`*_]|\b_|$)|[^ ](?= {2,}\n)))/,Se=/[\p{P}\p{S}]/u,rt=/[\s\p{P}\p{S}]/u,Kt=/[^\s\p{P}\p{S}]/u,Mn=y(/^((?![*_])punctSpace)/,"u").replace(/punctSpace/g,rt).getRegex(),Xt=/(?!~)[\p{P}\p{S}]/u,Hn=/(?!~)[\s\p{P}\p{S}]/u,zn=/(?:[^\s\p{P}\p{S}]|~)/u,Nn=y(/link|precode-code|html/,"g").replace("link",/\[(?:[^\[\]`]|(?<a>`+)[^`]+\k<a>(?!`))*?\]\((?:\\[\s\S]|[^\\\(\)]|\((?:\\[\s\S]|[^\\\(\)])*\))*\)/).replace("precode-",kn?"(?<!`)()":"(^^|[^`])").replace("code",/(?<b>`+)[^`]+\k<b>(?!`)/).replace("html",/<(?! )[^<>]*?>/).getRegex(),Jt=/^(?:\*+(?:((?!\*)punct)|[^\s*]))|^_+(?:((?!_)punct)|([^\s_]))/,Un=y(Jt,"u").replace(/punct/g,Se).getRegex(),On=y(Jt,"u").replace(/punct/g,Xt).getRegex(),Yt="^[^_*]*?__[^_*]*?\\*[^_*]*?(?=__)|[^*]+(?=[^*])|(?!\\*)punct(\\*+)(?=[\\s]|$)|notPunctSpace(\\*+)(?!\\*)(?=punctSpace|$)|(?!\\*)punctSpace(\\*+)(?=notPunctSpace)|[\\s](\\*+)(?!\\*)(?=punct)|(?!\\*)punct(\\*+)(?!\\*)(?=punct)|notPunctSpace(\\*+)(?=notPunctSpace)",Dn=y(Yt,"gu").replace(/notPunctSpace/g,Kt).replace(/punctSpace/g,rt).replace(/punct/g,Se).getRegex(),jn=y(Yt,"gu").replace(/notPunctSpace/g,zn).replace(/punctSpace/g,Hn).replace(/punct/g,Xt).getRegex(),qn=y("^[^_*]*?\\*\\*[^_*]*?_[^_*]*?(?=\\*\\*)|[^_]+(?=[^_])|(?!_)punct(_+)(?=[\\s]|$)|notPunctSpace(_+)(?!_)(?=punctSpace|$)|(?!_)punctSpace(_+)(?=notPunctSpace)|[\\s](_+)(?!_)(?=punct)|(?!_)punct(_+)(?!_)(?=punct)","gu").replace(/notPunctSpace/g,Kt).replace(/punctSpace/g,rt).replace(/punct/g,Se).getRegex(),Bn=y(/\\(punct)/,"gu").replace(/punct/g,Se).getRegex(),Fn=y(/^<(scheme:[^\s\x00-\x1f<>]*|email)>/).replace("scheme",/[a-zA-Z][a-zA-Z0-9+.-]{1,31}/).replace("email",/[a-zA-Z0-9.!#$%&'*+/=?^_`{|}~-]+(@)[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?(?:\.[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)+(?![-_])/).getRegex(),Wn=y(tt).replace("(?:-->|$)","-->").getRegex(),Vn=y("^comment|^</[a-zA-Z][\\w:-]*\\s*>|^<[a-zA-Z][\\w-]*(?:attribute)*?\\s*/?>|^<\\?[\\s\\S]*?\\?>|^<![a-zA-Z]+\\s[\\s\\S]*?>|^<!\\[CDATA\\[[\\s\\S]*?\\]\\]>").replace("comment",Wn).replace("attribute",/\s+[a-zA-Z:_][\w.:-]*(?:\s*=\s*"[^"]*"|\s*=\s*'[^']*'|\s*=\s*[^\s"'=<>`]+)?/).getRegex(),me=/(?:\[(?:\\[\s\S]|[^\[\]\\])*\]|\\[\s\S]|`+[^`]*?`+(?!`)|[^\[\]\\`])*?/,Zn=y(/^!?\[(label)\]\(\s*(href)(?:(?:[ \t]*(?:\n[ \t]*)?)(title))?\s*\)/).replace("label",me).replace("href",/<(?:\\.|[^\n<>\\])+>|[^ \t\n\x00-\x1f]*/).replace("title",/"(?:\\"?|[^"\\])*"|'(?:\\'?|[^'\\])*'|\((?:\\\)?|[^)\\])*\)/).getRegex(),en=y(/^!?\[(label)\]\[(ref)\]/).replace("label",me).replace("ref",et).getRegex(),tn=y(/^!?\[(ref)\](?:\[\])?/).replace("ref",et).getRegex(),Qn=y("reflink|nolink(?!\\()","g").replace("reflink",en).replace("nolink",tn).getRegex(),$t=/[hH][tT][tT][pP][sS]?|[fF][tT][pP]/,st={_backpedal:re,anyPunctuation:Bn,autolink:Fn,blockSkip:Nn,br:Gt,code:Pn,del:re,emStrongLDelim:Un,emStrongRDelimAst:Dn,emStrongRDelimUnd:qn,escape:An,link:Zn,nolink:tn,punctuation:Mn,reflink:en,reflinkSearch:Qn,tag:Vn,text:In,url:re},Gn={...st,link:y(/^!?\[(label)\]\((.*?)\)/).replace("label",me).getRegex(),reflink:y(/^!?\[(label)\]\s*\[([^\]]*)\]/).replace("label",me).getRegex()},Oe={...st,emStrongRDelimAst:jn,emStrongLDelim:On,url:y(/^((?:protocol):\/\/|www\.)(?:[a-zA-Z0-9\-]+\.?)+[^\s<]*|^email/).replace("protocol",$t).replace("email",/[A-Za-z0-9._+-]+(@)[a-zA-Z0-9-_]+(?:\.[a-zA-Z0-9-_]*[a-zA-Z0-9])+(?![-_])/).getRegex(),_backpedal:/(?:[^?!.,:;*_'"~()&]+|\([^)]*\)|&(?![a-zA-Z0-9]+;$)|[?!.,:;*_'"~)]+(?!$))+/,del:/^(~~?)(?=[^\s~])((?:\\[\s\S]|[^\\])*?(?:\\[\s\S]|[^\s~\\]))\1(?=[^~]|$)/,text:y(/^([`~]+|[^`~])(?:(?= {2,}\n)|(?=[a-zA-Z0-9.!#$%&'*+\/=?_`{\|}~-]+@)|[\s\S]*?(?:(?=[\\<!\[`*~_]|\b_|protocol:\/\/|www\.|$)|[^ ](?= {2,}\n)|[^a-zA-Z0-9.!#$%&'*+\/=?_`{\|}~-](?=[a-zA-Z0-9.!#$%&'*+\/=?_`{\|}~-]+@)))/).replace("protocol",$t).getRegex()},Kn={...Oe,br:y(Gt).replace("{2,}","*").getRegex(),text:y(Oe.text).replace("\\b_","\\b_| {2,}\\n").replace(/\{2,\}/g,"*").getRegex()},ue={normal:nt,gfm:Ln,pedantic:En},ee={normal:st,gfm:Oe,breaks:Kn,pedantic:Gn},Xn={"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"},bt=t=>Xn[t];function B(t,e){if(e){if(z.escapeTest.test(t))return t.replace(z.escapeReplace,bt)}else if(z.escapeTestNoEncode.test(t))return t.replace(z.escapeReplaceNoEncode,bt);return t}function wt(t){try{t=encodeURI(t).replace(z.percentDecode,"%")}catch{return null}return t}function xt(t,e){let n=t.replace(z.findPipe,(l,i,o)=>{let a=!1,u=i;for(;--u>=0&&o[u]==="\\";)a=!a;return a?"|":" |"}),s=n.split(z.splitPipe),r=0;if(s[0].trim()||s.shift(),s.length>0&&!s.at(-1)?.trim()&&s.pop(),e)if(s.length>e)s.splice(e);else for(;s.length<e;)s.push("");for(;r<s.length;r++)s[r]=s[r].trim().replace(z.slashPipe,"|");return s}function te(t,e,n){let s=t.length;if(s===0)return"";let r=0;for(;r<s&&t.charAt(s-r-1)===e;)r++;return t.slice(0,s-r)}function Jn(t,e){if(t.indexOf(e[1])===-1)return-1;let n=0;for(let s=0;s<t.length;s++)if(t[s]==="\\")s++;else if(t[s]===e[0])n++;else if(t[s]===e[1]&&(n--,n<0))return s;return n>0?-2:-1}function yt(t,e,n,s,r){let l=e.href,i=e.title||null,o=t[1].replace(r.other.outputLinkReplace,"$1");s.state.inLink=!0;let a={type:t[0].charAt(0)==="!"?"image":"link",raw:n,href:l,title:i,text:o,tokens:s.inlineTokens(o)};return s.state.inLink=!1,a}function Yn(t,e,n){let s=t.match(n.other.indentCodeCompensation);if(s===null)return e;let r=s[1];return e.split(`
`).map(l=>{let i=l.match(n.other.beginningSpace);if(i===null)return l;let[o]=i;return o.length>=r.length?l.slice(r.length):l}).join(`
`)}var ke=class{options;rules;lexer;constructor(t){this.options=t||K}space(t){let e=this.rules.block.newline.exec(t);if(e&&e[0].length>0)return{type:"space",raw:e[0]}}code(t){let e=this.rules.block.code.exec(t);if(e){let n=e[0].replace(this.rules.other.codeRemoveIndent,"");return{type:"code",raw:e[0],codeBlockStyle:"indented",text:this.options.pedantic?n:te(n,`
`)}}}fences(t){let e=this.rules.block.fences.exec(t);if(e){let n=e[0],s=Yn(n,e[3]||"",this.rules);return{type:"code",raw:n,lang:e[2]?e[2].trim().replace(this.rules.inline.anyPunctuation,"$1"):e[2],text:s}}}heading(t){let e=this.rules.block.heading.exec(t);if(e){let n=e[2].trim();if(this.rules.other.endingHash.test(n)){let s=te(n,"#");(this.options.pedantic||!s||this.rules.other.endingSpaceChar.test(s))&&(n=s.trim())}return{type:"heading",raw:e[0],depth:e[1].length,text:n,tokens:this.lexer.inline(n)}}}hr(t){let e=this.rules.block.hr.exec(t);if(e)return{type:"hr",raw:te(e[0],`
`)}}blockquote(t){let e=this.rules.block.blockquote.exec(t);if(e){let n=te(e[0],`
`).split(`
`),s="",r="",l=[];for(;n.length>0;){let i=!1,o=[],a;for(a=0;a<n.length;a++)if(this.rules.other.blockquoteStart.test(n[a]))o.push(n[a]),i=!0;else if(!i)o.push(n[a]);else break;n=n.slice(a);let u=o.join(`
`),p=u.replace(this.rules.other.blockquoteSetextReplace,`
    $1`).replace(this.rules.other.blockquoteSetextReplace2,"");s=s?`${s}
${u}`:u,r=r?`${r}
${p}`:p;let c=this.lexer.state.top;if(this.lexer.state.top=!0,this.lexer.blockTokens(p,l,!0),this.lexer.state.top=c,n.length===0)break;let d=l.at(-1);if(d?.type==="code")break;if(d?.type==="blockquote"){let g=d,k=g.raw+`
`+n.join(`
`),m=this.blockquote(k);l[l.length-1]=m,s=s.substring(0,s.length-g.raw.length)+m.raw,r=r.substring(0,r.length-g.text.length)+m.text;break}else if(d?.type==="list"){let g=d,k=g.raw+`
`+n.join(`
`),m=this.list(k);l[l.length-1]=m,s=s.substring(0,s.length-d.raw.length)+m.raw,r=r.substring(0,r.length-g.raw.length)+m.raw,n=k.substring(l.at(-1).raw.length).split(`
`);continue}}return{type:"blockquote",raw:s,tokens:l,text:r}}}list(t){let e=this.rules.block.list.exec(t);if(e){let n=e[1].trim(),s=n.length>1,r={type:"list",raw:"",ordered:s,start:s?+n.slice(0,-1):"",loose:!1,items:[]};n=s?`\\d{1,9}\\${n.slice(-1)}`:`\\${n}`,this.options.pedantic&&(n=s?n:"[*+-]");let l=this.rules.other.listItemRegex(n),i=!1;for(;t;){let a=!1,u="",p="";if(!(e=l.exec(t))||this.rules.block.hr.test(t))break;u=e[0],t=t.substring(u.length);let c=e[2].split(`
`,1)[0].replace(this.rules.other.listReplaceTabs,m=>" ".repeat(3*m.length)),d=t.split(`
`,1)[0],g=!c.trim(),k=0;if(this.options.pedantic?(k=2,p=c.trimStart()):g?k=e[1].length+1:(k=e[2].search(this.rules.other.nonSpaceChar),k=k>4?1:k,p=c.slice(k),k+=e[1].length),g&&this.rules.other.blankLine.test(d)&&(u+=d+`
`,t=t.substring(d.length+1),a=!0),!a){let m=this.rules.other.nextBulletRegex(k),_=this.rules.other.hrRegex(k),h=this.rules.other.fencesBeginRegex(k),f=this.rules.other.headingBeginRegex(k),$=this.rules.other.htmlBeginRegex(k);for(;t;){let x=t.split(`
`,1)[0],b;if(d=x,this.options.pedantic?(d=d.replace(this.rules.other.listReplaceNesting,"  "),b=d):b=d.replace(this.rules.other.tabCharGlobal,"    "),h.test(d)||f.test(d)||$.test(d)||m.test(d)||_.test(d))break;if(b.search(this.rules.other.nonSpaceChar)>=k||!d.trim())p+=`
`+b.slice(k);else{if(g||c.replace(this.rules.other.tabCharGlobal,"    ").search(this.rules.other.nonSpaceChar)>=4||h.test(c)||f.test(c)||_.test(c))break;p+=`
`+d}!g&&!d.trim()&&(g=!0),u+=x+`
`,t=t.substring(x.length+1),c=b.slice(k)}}r.loose||(i?r.loose=!0:this.rules.other.doubleBlankLine.test(u)&&(i=!0)),r.items.push({type:"list_item",raw:u,task:!!this.options.gfm&&this.rules.other.listIsTask.test(p),loose:!1,text:p,tokens:[]}),r.raw+=u}let o=r.items.at(-1);if(o)o.raw=o.raw.trimEnd(),o.text=o.text.trimEnd();else return;r.raw=r.raw.trimEnd();for(let a of r.items){if(this.lexer.state.top=!1,a.tokens=this.lexer.blockTokens(a.text,[]),a.task){if(a.text=a.text.replace(this.rules.other.listReplaceTask,""),a.tokens[0]?.type==="text"||a.tokens[0]?.type==="paragraph"){a.tokens[0].raw=a.tokens[0].raw.replace(this.rules.other.listReplaceTask,""),a.tokens[0].text=a.tokens[0].text.replace(this.rules.other.listReplaceTask,"");for(let p=this.lexer.inlineQueue.length-1;p>=0;p--)if(this.rules.other.listIsTask.test(this.lexer.inlineQueue[p].src)){this.lexer.inlineQueue[p].src=this.lexer.inlineQueue[p].src.replace(this.rules.other.listReplaceTask,"");break}}let u=this.rules.other.listTaskCheckbox.exec(a.raw);if(u){let p={type:"checkbox",raw:u[0]+" ",checked:u[0]!=="[ ]"};a.checked=p.checked,r.loose?a.tokens[0]&&["paragraph","text"].includes(a.tokens[0].type)&&"tokens"in a.tokens[0]&&a.tokens[0].tokens?(a.tokens[0].raw=p.raw+a.tokens[0].raw,a.tokens[0].text=p.raw+a.tokens[0].text,a.tokens[0].tokens.unshift(p)):a.tokens.unshift({type:"paragraph",raw:p.raw,text:p.raw,tokens:[p]}):a.tokens.unshift(p)}}if(!r.loose){let u=a.tokens.filter(c=>c.type==="space"),p=u.length>0&&u.some(c=>this.rules.other.anyLine.test(c.raw));r.loose=p}}if(r.loose)for(let a of r.items){a.loose=!0;for(let u of a.tokens)u.type==="text"&&(u.type="paragraph")}return r}}html(t){let e=this.rules.block.html.exec(t);if(e)return{type:"html",block:!0,raw:e[0],pre:e[1]==="pre"||e[1]==="script"||e[1]==="style",text:e[0]}}def(t){let e=this.rules.block.def.exec(t);if(e){let n=e[1].toLowerCase().replace(this.rules.other.multipleSpaceGlobal," "),s=e[2]?e[2].replace(this.rules.other.hrefBrackets,"$1").replace(this.rules.inline.anyPunctuation,"$1"):"",r=e[3]?e[3].substring(1,e[3].length-1).replace(this.rules.inline.anyPunctuation,"$1"):e[3];return{type:"def",tag:n,raw:e[0],href:s,title:r}}}table(t){let e=this.rules.block.table.exec(t);if(!e||!this.rules.other.tableDelimiter.test(e[2]))return;let n=xt(e[1]),s=e[2].replace(this.rules.other.tableAlignChars,"").split("|"),r=e[3]?.trim()?e[3].replace(this.rules.other.tableRowBlankLine,"").split(`
`):[],l={type:"table",raw:e[0],header:[],align:[],rows:[]};if(n.length===s.length){for(let i of s)this.rules.other.tableAlignRight.test(i)?l.align.push("right"):this.rules.other.tableAlignCenter.test(i)?l.align.push("center"):this.rules.other.tableAlignLeft.test(i)?l.align.push("left"):l.align.push(null);for(let i=0;i<n.length;i++)l.header.push({text:n[i],tokens:this.lexer.inline(n[i]),header:!0,align:l.align[i]});for(let i of r)l.rows.push(xt(i,l.header.length).map((o,a)=>({text:o,tokens:this.lexer.inline(o),header:!1,align:l.align[a]})));return l}}lheading(t){let e=this.rules.block.lheading.exec(t);if(e)return{type:"heading",raw:e[0],depth:e[2].charAt(0)==="="?1:2,text:e[1],tokens:this.lexer.inline(e[1])}}paragraph(t){let e=this.rules.block.paragraph.exec(t);if(e){let n=e[1].charAt(e[1].length-1)===`
`?e[1].slice(0,-1):e[1];return{type:"paragraph",raw:e[0],text:n,tokens:this.lexer.inline(n)}}}text(t){let e=this.rules.block.text.exec(t);if(e)return{type:"text",raw:e[0],text:e[0],tokens:this.lexer.inline(e[0])}}escape(t){let e=this.rules.inline.escape.exec(t);if(e)return{type:"escape",raw:e[0],text:e[1]}}tag(t){let e=this.rules.inline.tag.exec(t);if(e)return!this.lexer.state.inLink&&this.rules.other.startATag.test(e[0])?this.lexer.state.inLink=!0:this.lexer.state.inLink&&this.rules.other.endATag.test(e[0])&&(this.lexer.state.inLink=!1),!this.lexer.state.inRawBlock&&this.rules.other.startPreScriptTag.test(e[0])?this.lexer.state.inRawBlock=!0:this.lexer.state.inRawBlock&&this.rules.other.endPreScriptTag.test(e[0])&&(this.lexer.state.inRawBlock=!1),{type:"html",raw:e[0],inLink:this.lexer.state.inLink,inRawBlock:this.lexer.state.inRawBlock,block:!1,text:e[0]}}link(t){let e=this.rules.inline.link.exec(t);if(e){let n=e[2].trim();if(!this.options.pedantic&&this.rules.other.startAngleBracket.test(n)){if(!this.rules.other.endAngleBracket.test(n))return;let l=te(n.slice(0,-1),"\\");if((n.length-l.length)%2===0)return}else{let l=Jn(e[2],"()");if(l===-2)return;if(l>-1){let i=(e[0].indexOf("!")===0?5:4)+e[1].length+l;e[2]=e[2].substring(0,l),e[0]=e[0].substring(0,i).trim(),e[3]=""}}let s=e[2],r="";if(this.options.pedantic){let l=this.rules.other.pedanticHrefTitle.exec(s);l&&(s=l[1],r=l[3])}else r=e[3]?e[3].slice(1,-1):"";return s=s.trim(),this.rules.other.startAngleBracket.test(s)&&(this.options.pedantic&&!this.rules.other.endAngleBracket.test(n)?s=s.slice(1):s=s.slice(1,-1)),yt(e,{href:s&&s.replace(this.rules.inline.anyPunctuation,"$1"),title:r&&r.replace(this.rules.inline.anyPunctuation,"$1")},e[0],this.lexer,this.rules)}}reflink(t,e){let n;if((n=this.rules.inline.reflink.exec(t))||(n=this.rules.inline.nolink.exec(t))){let s=(n[2]||n[1]).replace(this.rules.other.multipleSpaceGlobal," "),r=e[s.toLowerCase()];if(!r){let l=n[0].charAt(0);return{type:"text",raw:l,text:l}}return yt(n,r,n[0],this.lexer,this.rules)}}emStrong(t,e,n=""){let s=this.rules.inline.emStrongLDelim.exec(t);if(!(!s||s[3]&&n.match(this.rules.other.unicodeAlphaNumeric))&&(!(s[1]||s[2])||!n||this.rules.inline.punctuation.exec(n))){let r=[...s[0]].length-1,l,i,o=r,a=0,u=s[0][0]==="*"?this.rules.inline.emStrongRDelimAst:this.rules.inline.emStrongRDelimUnd;for(u.lastIndex=0,e=e.slice(-1*t.length+r);(s=u.exec(e))!=null;){if(l=s[1]||s[2]||s[3]||s[4]||s[5]||s[6],!l)continue;if(i=[...l].length,s[3]||s[4]){o+=i;continue}else if((s[5]||s[6])&&r%3&&!((r+i)%3)){a+=i;continue}if(o-=i,o>0)continue;i=Math.min(i,i+o+a);let p=[...s[0]][0].length,c=t.slice(0,r+s.index+p+i);if(Math.min(r,i)%2){let g=c.slice(1,-1);return{type:"em",raw:c,text:g,tokens:this.lexer.inlineTokens(g)}}let d=c.slice(2,-2);return{type:"strong",raw:c,text:d,tokens:this.lexer.inlineTokens(d)}}}}codespan(t){let e=this.rules.inline.code.exec(t);if(e){let n=e[2].replace(this.rules.other.newLineCharGlobal," "),s=this.rules.other.nonSpaceChar.test(n),r=this.rules.other.startingSpaceChar.test(n)&&this.rules.other.endingSpaceChar.test(n);return s&&r&&(n=n.substring(1,n.length-1)),{type:"codespan",raw:e[0],text:n}}}br(t){let e=this.rules.inline.br.exec(t);if(e)return{type:"br",raw:e[0]}}del(t){let e=this.rules.inline.del.exec(t);if(e)return{type:"del",raw:e[0],text:e[2],tokens:this.lexer.inlineTokens(e[2])}}autolink(t){let e=this.rules.inline.autolink.exec(t);if(e){let n,s;return e[2]==="@"?(n=e[1],s="mailto:"+n):(n=e[1],s=n),{type:"link",raw:e[0],text:n,href:s,tokens:[{type:"text",raw:n,text:n}]}}}url(t){let e;if(e=this.rules.inline.url.exec(t)){let n,s;if(e[2]==="@")n=e[0],s="mailto:"+n;else{let r;do r=e[0],e[0]=this.rules.inline._backpedal.exec(e[0])?.[0]??"";while(r!==e[0]);n=e[0],e[1]==="www."?s="http://"+e[0]:s=e[0]}return{type:"link",raw:e[0],text:n,href:s,tokens:[{type:"text",raw:n,text:n}]}}}inlineText(t){let e=this.rules.inline.text.exec(t);if(e){let n=this.lexer.state.inRawBlock;return{type:"text",raw:e[0],text:e[0],escaped:n}}}},D=class De{tokens;options;state;inlineQueue;tokenizer;constructor(e){this.tokens=[],this.tokens.links=Object.create(null),this.options=e||K,this.options.tokenizer=this.options.tokenizer||new ke,this.tokenizer=this.options.tokenizer,this.tokenizer.options=this.options,this.tokenizer.lexer=this,this.inlineQueue=[],this.state={inLink:!1,inRawBlock:!1,top:!0};let n={other:z,block:ue.normal,inline:ee.normal};this.options.pedantic?(n.block=ue.pedantic,n.inline=ee.pedantic):this.options.gfm&&(n.block=ue.gfm,this.options.breaks?n.inline=ee.breaks:n.inline=ee.gfm),this.tokenizer.rules=n}static get rules(){return{block:ue,inline:ee}}static lex(e,n){return new De(n).lex(e)}static lexInline(e,n){return new De(n).inlineTokens(e)}lex(e){e=e.replace(z.carriageReturn,`
`),this.blockTokens(e,this.tokens);for(let n=0;n<this.inlineQueue.length;n++){let s=this.inlineQueue[n];this.inlineTokens(s.src,s.tokens)}return this.inlineQueue=[],this.tokens}blockTokens(e,n=[],s=!1){for(this.options.pedantic&&(e=e.replace(z.tabCharGlobal,"    ").replace(z.spaceLine,""));e;){let r;if(this.options.extensions?.block?.some(i=>(r=i.call({lexer:this},e,n))?(e=e.substring(r.raw.length),n.push(r),!0):!1))continue;if(r=this.tokenizer.space(e)){e=e.substring(r.raw.length);let i=n.at(-1);r.raw.length===1&&i!==void 0?i.raw+=`
`:n.push(r);continue}if(r=this.tokenizer.code(e)){e=e.substring(r.raw.length);let i=n.at(-1);i?.type==="paragraph"||i?.type==="text"?(i.raw+=(i.raw.endsWith(`
`)?"":`
`)+r.raw,i.text+=`
`+r.text,this.inlineQueue.at(-1).src=i.text):n.push(r);continue}if(r=this.tokenizer.fences(e)){e=e.substring(r.raw.length),n.push(r);continue}if(r=this.tokenizer.heading(e)){e=e.substring(r.raw.length),n.push(r);continue}if(r=this.tokenizer.hr(e)){e=e.substring(r.raw.length),n.push(r);continue}if(r=this.tokenizer.blockquote(e)){e=e.substring(r.raw.length),n.push(r);continue}if(r=this.tokenizer.list(e)){e=e.substring(r.raw.length),n.push(r);continue}if(r=this.tokenizer.html(e)){e=e.substring(r.raw.length),n.push(r);continue}if(r=this.tokenizer.def(e)){e=e.substring(r.raw.length);let i=n.at(-1);i?.type==="paragraph"||i?.type==="text"?(i.raw+=(i.raw.endsWith(`
`)?"":`
`)+r.raw,i.text+=`
`+r.raw,this.inlineQueue.at(-1).src=i.text):this.tokens.links[r.tag]||(this.tokens.links[r.tag]={href:r.href,title:r.title},n.push(r));continue}if(r=this.tokenizer.table(e)){e=e.substring(r.raw.length),n.push(r);continue}if(r=this.tokenizer.lheading(e)){e=e.substring(r.raw.length),n.push(r);continue}let l=e;if(this.options.extensions?.startBlock){let i=1/0,o=e.slice(1),a;this.options.extensions.startBlock.forEach(u=>{a=u.call({lexer:this},o),typeof a=="number"&&a>=0&&(i=Math.min(i,a))}),i<1/0&&i>=0&&(l=e.substring(0,i+1))}if(this.state.top&&(r=this.tokenizer.paragraph(l))){let i=n.at(-1);s&&i?.type==="paragraph"?(i.raw+=(i.raw.endsWith(`
`)?"":`
`)+r.raw,i.text+=`
`+r.text,this.inlineQueue.pop(),this.inlineQueue.at(-1).src=i.text):n.push(r),s=l.length!==e.length,e=e.substring(r.raw.length);continue}if(r=this.tokenizer.text(e)){e=e.substring(r.raw.length);let i=n.at(-1);i?.type==="text"?(i.raw+=(i.raw.endsWith(`
`)?"":`
`)+r.raw,i.text+=`
`+r.text,this.inlineQueue.pop(),this.inlineQueue.at(-1).src=i.text):n.push(r);continue}if(e){let i="Infinite loop on byte: "+e.charCodeAt(0);if(this.options.silent){console.error(i);break}else throw new Error(i)}}return this.state.top=!0,n}inline(e,n=[]){return this.inlineQueue.push({src:e,tokens:n}),n}inlineTokens(e,n=[]){let s=e,r=null;if(this.tokens.links){let a=Object.keys(this.tokens.links);if(a.length>0)for(;(r=this.tokenizer.rules.inline.reflinkSearch.exec(s))!=null;)a.includes(r[0].slice(r[0].lastIndexOf("[")+1,-1))&&(s=s.slice(0,r.index)+"["+"a".repeat(r[0].length-2)+"]"+s.slice(this.tokenizer.rules.inline.reflinkSearch.lastIndex))}for(;(r=this.tokenizer.rules.inline.anyPunctuation.exec(s))!=null;)s=s.slice(0,r.index)+"++"+s.slice(this.tokenizer.rules.inline.anyPunctuation.lastIndex);let l;for(;(r=this.tokenizer.rules.inline.blockSkip.exec(s))!=null;)l=r[2]?r[2].length:0,s=s.slice(0,r.index+l)+"["+"a".repeat(r[0].length-l-2)+"]"+s.slice(this.tokenizer.rules.inline.blockSkip.lastIndex);s=this.options.hooks?.emStrongMask?.call({lexer:this},s)??s;let i=!1,o="";for(;e;){i||(o=""),i=!1;let a;if(this.options.extensions?.inline?.some(p=>(a=p.call({lexer:this},e,n))?(e=e.substring(a.raw.length),n.push(a),!0):!1))continue;if(a=this.tokenizer.escape(e)){e=e.substring(a.raw.length),n.push(a);continue}if(a=this.tokenizer.tag(e)){e=e.substring(a.raw.length),n.push(a);continue}if(a=this.tokenizer.link(e)){e=e.substring(a.raw.length),n.push(a);continue}if(a=this.tokenizer.reflink(e,this.tokens.links)){e=e.substring(a.raw.length);let p=n.at(-1);a.type==="text"&&p?.type==="text"?(p.raw+=a.raw,p.text+=a.text):n.push(a);continue}if(a=this.tokenizer.emStrong(e,s,o)){e=e.substring(a.raw.length),n.push(a);continue}if(a=this.tokenizer.codespan(e)){e=e.substring(a.raw.length),n.push(a);continue}if(a=this.tokenizer.br(e)){e=e.substring(a.raw.length),n.push(a);continue}if(a=this.tokenizer.del(e)){e=e.substring(a.raw.length),n.push(a);continue}if(a=this.tokenizer.autolink(e)){e=e.substring(a.raw.length),n.push(a);continue}if(!this.state.inLink&&(a=this.tokenizer.url(e))){e=e.substring(a.raw.length),n.push(a);continue}let u=e;if(this.options.extensions?.startInline){let p=1/0,c=e.slice(1),d;this.options.extensions.startInline.forEach(g=>{d=g.call({lexer:this},c),typeof d=="number"&&d>=0&&(p=Math.min(p,d))}),p<1/0&&p>=0&&(u=e.substring(0,p+1))}if(a=this.tokenizer.inlineText(u)){e=e.substring(a.raw.length),a.raw.slice(-1)!=="_"&&(o=a.raw.slice(-1)),i=!0;let p=n.at(-1);p?.type==="text"?(p.raw+=a.raw,p.text+=a.text):n.push(a);continue}if(e){let p="Infinite loop on byte: "+e.charCodeAt(0);if(this.options.silent){console.error(p);break}else throw new Error(p)}}return n}},ve=class{options;parser;constructor(t){this.options=t||K}space(t){return""}code({text:t,lang:e,escaped:n}){let s=(e||"").match(z.notSpaceStart)?.[0],r=t.replace(z.endingNewline,"")+`
`;return s?'<pre><code class="language-'+B(s)+'">'+(n?r:B(r,!0))+`</code></pre>
`:"<pre><code>"+(n?r:B(r,!0))+`</code></pre>
`}blockquote({tokens:t}){return`<blockquote>
${this.parser.parse(t)}</blockquote>
`}html({text:t}){return t}def(t){return""}heading({tokens:t,depth:e}){return`<h${e}>${this.parser.parseInline(t)}</h${e}>
`}hr(t){return`<hr>
`}list(t){let e=t.ordered,n=t.start,s="";for(let i=0;i<t.items.length;i++){let o=t.items[i];s+=this.listitem(o)}let r=e?"ol":"ul",l=e&&n!==1?' start="'+n+'"':"";return"<"+r+l+`>
`+s+"</"+r+`>
`}listitem(t){return`<li>${this.parser.parse(t.tokens)}</li>
`}checkbox({checked:t}){return"<input "+(t?'checked="" ':"")+'disabled="" type="checkbox"> '}paragraph({tokens:t}){return`<p>${this.parser.parseInline(t)}</p>
`}table(t){let e="",n="";for(let r=0;r<t.header.length;r++)n+=this.tablecell(t.header[r]);e+=this.tablerow({text:n});let s="";for(let r=0;r<t.rows.length;r++){let l=t.rows[r];n="";for(let i=0;i<l.length;i++)n+=this.tablecell(l[i]);s+=this.tablerow({text:n})}return s&&(s=`<tbody>${s}</tbody>`),`<table>
<thead>
`+e+`</thead>
`+s+`</table>
`}tablerow({text:t}){return`<tr>
${t}</tr>
`}tablecell(t){let e=this.parser.parseInline(t.tokens),n=t.header?"th":"td";return(t.align?`<${n} align="${t.align}">`:`<${n}>`)+e+`</${n}>
`}strong({tokens:t}){return`<strong>${this.parser.parseInline(t)}</strong>`}em({tokens:t}){return`<em>${this.parser.parseInline(t)}</em>`}codespan({text:t}){return`<code>${B(t,!0)}</code>`}br(t){return"<br>"}del({tokens:t}){return`<del>${this.parser.parseInline(t)}</del>`}link({href:t,title:e,tokens:n}){let s=this.parser.parseInline(n),r=wt(t);if(r===null)return s;t=r;let l='<a href="'+t+'"';return e&&(l+=' title="'+B(e)+'"'),l+=">"+s+"</a>",l}image({href:t,title:e,text:n,tokens:s}){s&&(n=this.parser.parseInline(s,this.parser.textRenderer));let r=wt(t);if(r===null)return B(n);t=r;let l=`<img src="${t}" alt="${n}"`;return e&&(l+=` title="${B(e)}"`),l+=">",l}text(t){return"tokens"in t&&t.tokens?this.parser.parseInline(t.tokens):"escaped"in t&&t.escaped?t.text:B(t.text)}},lt=class{strong({text:e}){return e}em({text:e}){return e}codespan({text:e}){return e}del({text:e}){return e}html({text:e}){return e}text({text:e}){return e}link({text:e}){return""+e}image({text:e}){return""+e}br(){return""}checkbox({raw:e}){return e}},j=class je{options;renderer;textRenderer;constructor(e){this.options=e||K,this.options.renderer=this.options.renderer||new ve,this.renderer=this.options.renderer,this.renderer.options=this.options,this.renderer.parser=this,this.textRenderer=new lt}static parse(e,n){return new je(n).parse(e)}static parseInline(e,n){return new je(n).parseInline(e)}parse(e){let n="";for(let s=0;s<e.length;s++){let r=e[s];if(this.options.extensions?.renderers?.[r.type]){let i=r,o=this.options.extensions.renderers[i.type].call({parser:this},i);if(o!==!1||!["space","hr","heading","code","table","blockquote","list","html","def","paragraph","text"].includes(i.type)){n+=o||"";continue}}let l=r;switch(l.type){case"space":{n+=this.renderer.space(l);break}case"hr":{n+=this.renderer.hr(l);break}case"heading":{n+=this.renderer.heading(l);break}case"code":{n+=this.renderer.code(l);break}case"table":{n+=this.renderer.table(l);break}case"blockquote":{n+=this.renderer.blockquote(l);break}case"list":{n+=this.renderer.list(l);break}case"checkbox":{n+=this.renderer.checkbox(l);break}case"html":{n+=this.renderer.html(l);break}case"def":{n+=this.renderer.def(l);break}case"paragraph":{n+=this.renderer.paragraph(l);break}case"text":{n+=this.renderer.text(l);break}default:{let i='Token with "'+l.type+'" type was not found.';if(this.options.silent)return console.error(i),"";throw new Error(i)}}}return n}parseInline(e,n=this.renderer){let s="";for(let r=0;r<e.length;r++){let l=e[r];if(this.options.extensions?.renderers?.[l.type]){let o=this.options.extensions.renderers[l.type].call({parser:this},l);if(o!==!1||!["escape","html","link","image","strong","em","codespan","br","del","text"].includes(l.type)){s+=o||"";continue}}let i=l;switch(i.type){case"escape":{s+=n.text(i);break}case"html":{s+=n.html(i);break}case"link":{s+=n.link(i);break}case"image":{s+=n.image(i);break}case"checkbox":{s+=n.checkbox(i);break}case"strong":{s+=n.strong(i);break}case"em":{s+=n.em(i);break}case"codespan":{s+=n.codespan(i);break}case"br":{s+=n.br(i);break}case"del":{s+=n.del(i);break}case"text":{s+=n.text(i);break}default:{let o='Token with "'+i.type+'" type was not found.';if(this.options.silent)return console.error(o),"";throw new Error(o)}}}return s}},ne=class{options;block;constructor(e){this.options=e||K}static passThroughHooks=new Set(["preprocess","postprocess","processAllTokens","emStrongMask"]);static passThroughHooksRespectAsync=new Set(["preprocess","postprocess","processAllTokens"]);preprocess(e){return e}postprocess(e){return e}processAllTokens(e){return e}emStrongMask(e){return e}provideLexer(){return this.block?D.lex:D.lexInline}provideParser(){return this.block?j.parse:j.parseInline}},er=class{defaults=Xe();options=this.setOptions;parse=this.parseMarkdown(!0);parseInline=this.parseMarkdown(!1);Parser=j;Renderer=ve;TextRenderer=lt;Lexer=D;Tokenizer=ke;Hooks=ne;constructor(...t){this.use(...t)}walkTokens(t,e){let n=[];for(let s of t)switch(n=n.concat(e.call(this,s)),s.type){case"table":{let r=s;for(let l of r.header)n=n.concat(this.walkTokens(l.tokens,e));for(let l of r.rows)for(let i of l)n=n.concat(this.walkTokens(i.tokens,e));break}case"list":{let r=s;n=n.concat(this.walkTokens(r.items,e));break}default:{let r=s;this.defaults.extensions?.childTokens?.[r.type]?this.defaults.extensions.childTokens[r.type].forEach(l=>{let i=r[l].flat(1/0);n=n.concat(this.walkTokens(i,e))}):r.tokens&&(n=n.concat(this.walkTokens(r.tokens,e)))}}return n}use(...t){let e=this.defaults.extensions||{renderers:{},childTokens:{}};return t.forEach(n=>{let s={...n};if(s.async=this.defaults.async||s.async||!1,n.extensions&&(n.extensions.forEach(r=>{if(!r.name)throw new Error("extension name required");if("renderer"in r){let l=e.renderers[r.name];l?e.renderers[r.name]=function(...i){let o=r.renderer.apply(this,i);return o===!1&&(o=l.apply(this,i)),o}:e.renderers[r.name]=r.renderer}if("tokenizer"in r){if(!r.level||r.level!=="block"&&r.level!=="inline")throw new Error("extension level must be 'block' or 'inline'");let l=e[r.level];l?l.unshift(r.tokenizer):e[r.level]=[r.tokenizer],r.start&&(r.level==="block"?e.startBlock?e.startBlock.push(r.start):e.startBlock=[r.start]:r.level==="inline"&&(e.startInline?e.startInline.push(r.start):e.startInline=[r.start]))}"childTokens"in r&&r.childTokens&&(e.childTokens[r.name]=r.childTokens)}),s.extensions=e),n.renderer){let r=this.defaults.renderer||new ve(this.defaults);for(let l in n.renderer){if(!(l in r))throw new Error(`renderer '${l}' does not exist`);if(["options","parser"].includes(l))continue;let i=l,o=n.renderer[i],a=r[i];r[i]=(...u)=>{let p=o.apply(r,u);return p===!1&&(p=a.apply(r,u)),p||""}}s.renderer=r}if(n.tokenizer){let r=this.defaults.tokenizer||new ke(this.defaults);for(let l in n.tokenizer){if(!(l in r))throw new Error(`tokenizer '${l}' does not exist`);if(["options","rules","lexer"].includes(l))continue;let i=l,o=n.tokenizer[i],a=r[i];r[i]=(...u)=>{let p=o.apply(r,u);return p===!1&&(p=a.apply(r,u)),p}}s.tokenizer=r}if(n.hooks){let r=this.defaults.hooks||new ne;for(let l in n.hooks){if(!(l in r))throw new Error(`hook '${l}' does not exist`);if(["options","block"].includes(l))continue;let i=l,o=n.hooks[i],a=r[i];ne.passThroughHooks.has(l)?r[i]=u=>{if(this.defaults.async&&ne.passThroughHooksRespectAsync.has(l))return(async()=>{let c=await o.call(r,u);return a.call(r,c)})();let p=o.call(r,u);return a.call(r,p)}:r[i]=(...u)=>{if(this.defaults.async)return(async()=>{let c=await o.apply(r,u);return c===!1&&(c=await a.apply(r,u)),c})();let p=o.apply(r,u);return p===!1&&(p=a.apply(r,u)),p}}s.hooks=r}if(n.walkTokens){let r=this.defaults.walkTokens,l=n.walkTokens;s.walkTokens=function(i){let o=[];return o.push(l.call(this,i)),r&&(o=o.concat(r.call(this,i))),o}}this.defaults={...this.defaults,...s}}),this}setOptions(t){return this.defaults={...this.defaults,...t},this}lexer(t,e){return D.lex(t,e??this.defaults)}parser(t,e){return j.parse(t,e??this.defaults)}parseMarkdown(t){return(e,n)=>{let s={...n},r={...this.defaults,...s},l=this.onError(!!r.silent,!!r.async);if(this.defaults.async===!0&&s.async===!1)return l(new Error("marked(): The async option was set to true by an extension. Remove async: false from the parse options object to return a Promise."));if(typeof e>"u"||e===null)return l(new Error("marked(): input parameter is undefined or null"));if(typeof e!="string")return l(new Error("marked(): input parameter is of type "+Object.prototype.toString.call(e)+", string expected"));if(r.hooks&&(r.hooks.options=r,r.hooks.block=t),r.async)return(async()=>{let i=r.hooks?await r.hooks.preprocess(e):e,o=await(r.hooks?await r.hooks.provideLexer():t?D.lex:D.lexInline)(i,r),a=r.hooks?await r.hooks.processAllTokens(o):o;r.walkTokens&&await Promise.all(this.walkTokens(a,r.walkTokens));let u=await(r.hooks?await r.hooks.provideParser():t?j.parse:j.parseInline)(a,r);return r.hooks?await r.hooks.postprocess(u):u})().catch(l);try{r.hooks&&(e=r.hooks.preprocess(e));let i=(r.hooks?r.hooks.provideLexer():t?D.lex:D.lexInline)(e,r);r.hooks&&(i=r.hooks.processAllTokens(i)),r.walkTokens&&this.walkTokens(i,r.walkTokens);let o=(r.hooks?r.hooks.provideParser():t?j.parse:j.parseInline)(i,r);return r.hooks&&(o=r.hooks.postprocess(o)),o}catch(i){return l(i)}}}onError(t,e){return n=>{if(n.message+=`
Please report this to https://github.com/markedjs/marked.`,t){let s="<p>An error occurred:</p><pre>"+B(n.message+"",!0)+"</pre>";return e?Promise.resolve(s):s}if(e)return Promise.reject(n);throw n}}},G=new er;function R(t,e){return G.parse(t,e)}R.options=R.setOptions=function(t){return G.setOptions(t),R.defaults=G.defaults,Wt(R.defaults),R};R.getDefaults=Xe;R.defaults=K;R.use=function(...t){return G.use(...t),R.defaults=G.defaults,Wt(R.defaults),R};R.walkTokens=function(t,e){return G.walkTokens(t,e)};R.parseInline=G.parseInline;R.Parser=j;R.parser=j.parse;R.Renderer=ve;R.TextRenderer=lt;R.Lexer=D;R.lexer=D.lex;R.Tokenizer=ke;R.Hooks=ne;R.parse=R;R.options;R.setOptions;R.use;R.walkTokens;R.parseInline;j.parse;D.lex;const tr="2.4.7",nr=["ada","agda","asciidoc","asm","awk","bash","batch","c","c-sharp","caddy","capnp","clojure","cmake","commonlisp","cpp","css","d","dart","devicetree","diff","dockerfile","dot","elisp","elixir","elm","erlang","fish","fsharp","gleam","glsl","go","graphql","groovy","haskell","hcl","hlsl","html","idris","ini","java","javascript","jinja2","jq","json","julia","kdl","kotlin","lean","lua","markdown","matlab","meson","nginx","ninja","nix","objc","ocaml","perl","php","postscript","powershell","prolog","python","query","r","rescript","ron","ruby","rust","scala","scheme","scss","sparql","sql","ssh-config","starlark","svelte","swift","textproto","thrift","tlaplus","toml","tsx","typescript","typst","uiua","vb","verilog","vhdl","vim","vue","wit","x86asm","xml","yaml","yuri","zig","zsh"],rr={manual:!1,theme:"one-dark",selector:"pre code",cdn:"jsdelivr",version:tr,pluginsUrl:"",hostUrl:""};let Ce=null,Le=null,W={...rr};const qe=new Map,nn=new Set(nr);let ae=null,Ee=null;async function sr(){if(W.pluginsUrl)return Ee||(Ee=(async()=>{console.debug(`[arborium] Loading local plugins manifest from: ${W.pluginsUrl}`);const t=await fetch(W.pluginsUrl);if(!t.ok)throw new Error(`Failed to load plugins.json: ${t.status}`);ae=await t.json(),console.debug(`[arborium] Loaded local manifest with ${ae?.entries.length} entries`)})(),Ee)}function lr(t){if(ae){const r=ae.entries.find(l=>l.language===t);if(r)return r.local_js.substring(0,r.local_js.lastIndexOf("/"))}const e=W.cdn,n=W.version;let s;return e==="jsdelivr"?s="https://cdn.jsdelivr.net/npm":e==="unpkg"?s="https://unpkg.com":s=e,`${s}/@arborium/${t}@${n}`}async function rn(t){const e=qe.get(t);if(e)return console.debug(`[arborium] Grammar '${t}' found in cache`),e;if(await sr(),!nn.has(t)&&!ae?.entries.some(n=>n.language===t))return console.debug(`[arborium] Grammar '${t}' not available`),null;try{const n=lr(t),s=`${n}/grammar.js`,r=`${n}/grammar_bg.wasm`;console.debug(`[arborium] Loading grammar '${t}' from ${s}`);const l=await import(s);await l.default(r);const i=l.language_id();i!==t&&console.warn(`[arborium] Language ID mismatch: expected '${t}', got '${i}'`);const o=l.injection_languages(),a={languageId:t,injectionLanguages:o,module:l,parse:u=>{const p=l.create_session();try{l.set_text(p,u);const c=l.parse(p);return{spans:c.spans||[],injections:c.injections||[]}}catch(c){return console.error("[arborium] Parse error:",c),{spans:[],injections:[]}}finally{l.free_session(p)}}};return qe.set(t,a),console.debug(`[arborium] Grammar '${t}' loaded successfully`),a}catch(n){return console.error(`[arborium] Failed to load grammar '${t}':`,n),null}}const Ae=new Map;let ir=1;function ar(){window.arboriumHost={isLanguageAvailable(t){return nn.has(t)||qe.has(t)},async loadGrammar(t){const e=await rn(t);if(!e)return 0;for(const[s,r]of Ae)if(r===e)return s;const n=ir++;return Ae.set(n,e),n},parse(t,e){const n=Ae.get(t);return n?n.parse(e):{spans:[],injections:[]}}}}function or(){if(W.hostUrl)return W.hostUrl;const t=W.cdn,e=W.version;let n;t==="jsdelivr"?n="https://cdn.jsdelivr.net/npm":t==="unpkg"?n="https://unpkg.com":n=t;const s=e==="latest"?"":`@${e}`;return`${n}/@arborium/arborium${s}/dist`}async function cr(){return Ce||Le||(Le=(async()=>{ar();const t=or(),e=`${t}/arborium_host.js`,n=`${t}/arborium_host_bg.wasm`;console.debug(`[arborium] Loading host from ${e}`);try{const s=await import(e);return await s.default(n),Ce={highlight:s.highlight,isLanguageAvailable:s.isLanguageAvailable},console.debug("[arborium] Host loaded successfully"),Ce}catch(s){return console.error("[arborium] Failed to load host:",s),null}})(),Le)}async function ur(t,e,n){const s=await cr();if(s)try{return s.highlight(t,e)}catch(i){console.warn("Host highlight failed, falling back to JS:",i)}const r=await rn(t);if(!r)return fe(e);const l=r.parse(e);return pr(e,l.spans)}function pr(t,e){const n=[...e].sort((l,i)=>l.start-i.start);let s="",r=0;for(const l of n){if(l.start<r)continue;l.start>r&&(s+=fe(t.slice(r,l.start)));const i=hr(l.capture),o=fe(t.slice(l.start,l.end));i?s+=`<a-${i}>${o}</a-${i}>`:s+=o,r=l.end}return r<t.length&&(s+=fe(t.slice(r))),s}function hr(t){return t.startsWith("keyword")||t==="include"||t==="conditional"?"k":t.startsWith("function")||t.startsWith("method")?"f":t.startsWith("string")||t==="character"?"s":t.startsWith("comment")?"c":t.startsWith("type")?"t":t.startsWith("variable")?"v":t.startsWith("number")||t==="float"?"n":t.startsWith("operator")?"o":t.startsWith("punctuation")?"p":t.startsWith("tag")?"tg":t.startsWith("attribute")?"at":null}function fe(t){return t.replace(/&/g,"&amp;").replace(/</g,"&lt;").replace(/>/g,"&gt;").replace(/"/g,"&quot;")}const v=mn.bind(Mt),Pe=new Map;async function X(t){const e=await fetch(t);if(!e.ok)throw new Error(`HTTP ${e.status}`);return e.json()}function St(){const t=window.location.pathname,e=new URLSearchParams(window.location.search);if(t==="/sources"||t.startsWith("/sources/")){const n=t.length>9?t.slice(9):"",s=e.get("context");if(n){const r=n.lastIndexOf(":");if(r!==-1){const l=n.slice(0,r),i=parseInt(n.slice(r+1),10);return{view:"sources",file:l,line:isNaN(i)?null:i,context:s}}return{view:"sources",file:n,line:null,context:s}}return{view:"sources",file:null,line:null,context:s}}return t.startsWith("/spec")?{view:"spec",rule:(t.length>5?t.slice(6):e.get("rule"))??null}:{view:"coverage",filter:e.get("filter"),level:e.get("level")}}function sn(t,e={}){if(t==="sources"){const{file:r,line:l,context:i}=e;let o="/sources";return r&&(o=l?`/sources/${r}:${l}`:`/sources/${r}`),i&&(o+=`?context=${encodeURIComponent(i)}`),o}if(t==="spec"){const{rule:r}=e;return r?`/spec/${r}`:"/spec"}const n=new URLSearchParams;e.filter&&n.set("filter",e.filter),e.level&&e.level!=="all"&&n.set("level",e.level);const s=n.toString();return`/coverage${s?"?"+s:""}`}function Z(t,e={},n=!1){const s=sn(t,e);n?history.replaceState(null,"",s):history.pushState(null,"",s),window.dispatchEvent(new PopStateEvent("popstate"))}function dr(){const[t,e]=M(St);return L(()=>{const n=()=>e(St());return window.addEventListener("popstate",n),()=>window.removeEventListener("popstate",n)},[]),t}function fr(){const[t,e]=M(null),[n,s]=M(null),[r,l]=M(null),i=N(async()=>{try{const[o,a,u]=await Promise.all([X("/api/config"),X("/api/forward"),X("/api/reverse")]);e({config:o,forward:a,reverse:u}),s(null)}catch(o){s(o instanceof Error?o.message:String(o))}},[]);return L(()=>{i()},[i]),L(()=>{let o=!0,a=null;async function u(){if(o){try{const p=await X("/api/version");a!==null&&p.version!==a&&(console.log(`Version changed: ${a} -> ${p.version}, refetching...`),await i()),a=p.version,l(p.version)}catch(p){console.warn("Version poll failed:",p)}o&&setTimeout(u,500)}}return u(),()=>{o=!1}},[i]),{data:t,error:n,version:r,refetch:i}}function gr(t){const[e,n]=M(null);return L(()=>{if(!t){n(null);return}X("/api/file?path="+encodeURIComponent(t)).then(n).catch(s=>{console.error("Failed to load file:",s),n(null)})},[t]),e}function _r(t){const[e,n]=M(null);return L(()=>{if(!t){n(null);return}X("/api/spec?name="+encodeURIComponent(t)).then(n).catch(s=>{console.error("Failed to load spec:",s),n(null)})},[t]),e}function mr(t){const e={name:"",children:{},files:[],totalUnits:0,coveredUnits:0};for(const s of t){const r=s.path.split("/");let l=e;for(let i=0;i<r.length-1;i++){const o=r[i];l.children[o]||(l.children[o]={name:o,children:{},files:[],totalUnits:0,coveredUnits:0}),l=l.children[o]}l.files.push({...s,name:r[r.length-1]})}function n(s){let r=0,l=0;for(const i of s.files)r+=i.totalUnits||0,l+=i.coveredUnits||0;for(const i of Object.values(s.children))n(i),r+=i.totalUnits,l+=i.coveredUnits;s.totalUnits=r,s.coveredUnits=l}return n(e),e}function ln(t,e){if(e===0)return{class:"none",text:"-"};const n=t/e*100;return n===100?{class:"full",text:"100%"}:n>=50?{class:"partial",text:Math.round(n)+"%"}:{class:"none",text:Math.round(n)+"%"}}function Be(t){return t>=80?"good":t>=50?"warn":"bad"}function kr(t){if(!t)return"";let e=t.replace(/&/g,"&amp;").replace(/</g,"&lt;").replace(/>/g,"&gt;"),n=!1,s="";for(const r of e)r==="`"?n?(s+="</code>",n=!1):(s+="<code>",n=!0):s+=r;return n&&(s+="</code>"),e=s,e=e.replace(/\bMUST NOT\b/g,"<kw-must-not>MUST NOT</kw-must-not>").replace(/\bSHALL NOT\b/g,"<kw-shall-not>SHALL NOT</kw-shall-not>").replace(/\bSHOULD NOT\b/g,"<kw-should-not>SHOULD NOT</kw-should-not>").replace(/\bNOT RECOMMENDED\b/g,"<kw-not-recommended>NOT RECOMMENDED</kw-not-recommended>").replace(/\bMUST\b/g,"<kw-must>MUST</kw-must>").replace(/\bREQUIRED\b/g,"<kw-required>REQUIRED</kw-required>").replace(/\bSHALL\b/g,"<kw-shall>SHALL</kw-shall>").replace(/\bSHOULD\b/g,"<kw-should>SHOULD</kw-should>").replace(/\bRECOMMENDED\b/g,"<kw-recommended>RECOMMENDED</kw-recommended>").replace(/\bMAY\b/g,"<kw-may>MAY</kw-may>").replace(/\bOPTIONAL\b/g,"<kw-optional>OPTIONAL</kw-optional>"),e}async function vr(t,e="rust"){const n=`${e}:${t}`;if(Pe.has(n))return Pe.get(n);try{const s=await ur(e,t);return Pe.set(n,s),s}catch(s){return console.warn("Highlight failed:",s),t.replace(/&/g,"&amp;").replace(/</g,"&lt;").replace(/>/g,"&gt;")}}function $r(t){const s=new DOMParser().parseFromString(`<div>${t}</div>`,"text/html").body.firstChild,r=[];let l="",i=[];function o(a){if(a.nodeType===Node.TEXT_NODE){const u=a.textContent;for(const p of u)if(p===`
`){for(let c=i.length-1;c>=0;c--)l+=`</${i[c].tag}>`;r.push(l),l="";for(const c of i)l+=`<${c.tag}${c.attrs}>`}else l+=p==="<"?"&lt;":p===">"?"&gt;":p==="&"?"&amp;":p}else if(a.nodeType===Node.ELEMENT_NODE){const u=a.tagName.toLowerCase();let p="";for(const c of a.attributes)p+=` ${c.name}="${c.value.replace(/"/g,"&quot;")}"`;l+=`<${u}${p}>`,i.push({tag:u,attrs:p});for(const c of a.childNodes)o(c);i.pop(),l+=`</${u}>`}}for(const a of s.childNodes)o(a);return l&&r.push(l),r}function br(t,e="rust"){const[n,s]=M(null);return L(()=>{if(!t){s(null);return}let r=!1;return vr(t,e).then(l=>{if(r)return;const i=l.match(/<pre[^>]*><code[^>]*>([\s\S]*)<\/code><\/pre>/),o=i?i[1]:l;s($r(o))}),()=>{r=!0}},[t,e]),n}const wr=typeof navigator<"u"&&navigator.platform.toUpperCase().indexOf("MAC")>=0,xr=wr?"⌘":"Ctrl";function yr(){const{data:t,error:e}=fr(),n=dr(),[s,r]=M(""),[l,i]=M({}),[o,a]=M(!1);if(e)return v`<div class="loading">Error: ${e}</div>`;if(!t)return v`<div class="loading">Loading...</div>`;const{config:u,forward:p,reverse:c}=t,d=n.view,g=n.view==="sources"?n.file:null,k=n.view==="sources"?n.line:null,m=n.view==="sources"?n.context:null,_=n.view==="spec"?n.rule:null,h=n.view==="coverage"?n.filter:null,$=(n.view==="coverage"?n.level:null)||"all",x=N(S=>{Z("coverage",{filter:h,level:S},!1)},[h]),b=N(S=>{Z(S,{},!1)},[]),w=N((S,C=null,E=null)=>{Z("sources",{file:S,line:C,context:E},!1)},[]),I=N(S=>{Z("spec",{rule:S},!1)},[]),H=N(()=>{Z("sources",{file:g,line:k,context:null},!0)},[g,k]);L(()=>{const S=C=>{(C.metaKey||C.ctrlKey)&&C.key==="k"&&(C.preventDefault(),a(!0)),C.key==="Escape"&&a(!1)};return window.addEventListener("keydown",S),()=>window.removeEventListener("keydown",S)},[]);const U=N(S=>{a(!1),S.kind==="rule"?Z("spec",{rule:S.id},!1):Z("sources",{file:S.id,line:S.line},!1)},[]),O=N(S=>{Z("coverage",{filter:S,level:$},!1)},[$]);return v`
    <div class="layout">
      <${Lr}
        view=${d}
        onViewChange=${b}
        onOpenSearch=${()=>a(!0)}
      />

      ${o&&v`
        <${Cr}
          onClose=${()=>a(!1)}
          onSelect=${U}
        />
      `}

      ${d==="coverage"&&v`
        <${Ar}
          data=${p}
          config=${u}
          search=${s}
          onSearchChange=${r}
          level=${$}
          onLevelChange=${x}
          filter=${h}
          onFilterChange=${O}
          onSelectRule=${I}
          onSelectFile=${w}
        />
      `}

      ${d==="sources"&&v`
        <${Pr}
          data=${c}
          forward=${p}
          config=${u}
          search=${s}
          onSearchChange=${r}
          selectedFile=${g}
          selectedLine=${k}
          ruleContext=${m}
          onSelectFile=${w}
          onSelectRule=${I}
          onClearContext=${H}
        />
      `}

      ${d==="spec"&&v`
        <${Mr}
          config=${u}
          forward=${p}
          selectedRule=${_}
          onSelectRule=${I}
          onSelectFile=${w}
          scrollPosition=${l.spec||0}
          onScrollChange=${S=>i(C=>({...C,spec:S}))}
        />
      `}
    </div>
  `}const Sr='<svg class="editor-icon-svg" viewBox="0 0 128 128"><path fill="currentColor" d="M12 8a4 4 0 0 0-4 4v88H0V12C0 5.373 5.373 0 12 0h107.172c5.345 0 8.022 6.463 4.242 10.243L57.407 76.25H76V68h8v10.028a4 4 0 0 1-4 4H49.97l-13.727 13.729H98V56h8v47.757a8 8 0 0 1-8 8H27.657l-13.97 13.97H116a4 4 0 0 0 4-4V28h8v93.757c0 6.627-5.373 12-12 12H8.828c-5.345 0-8.022-6.463-4.242-10.243L70.343 57.757H52v8h-8V55.728a4 4 0 0 1 4-4h30.086l13.727-13.728H30V78h-8V30.243a8 8 0 0 1 8-8h70.343l13.97-13.971H12z"/></svg>',Rr={zed:{name:"Zed",urlTemplate:(t,e)=>`zed://file/${t}:${e}`,icon:Sr},vscode:{name:"VS Code",urlTemplate:(t,e)=>`vscode://file/${t}:${e}`,devicon:"devicon-vscode-plain"},idea:{name:"IntelliJ",urlTemplate:(t,e)=>`idea://open?file=${t}&line=${e}`,devicon:"devicon-intellij-plain"},vim:{name:"Vim",urlTemplate:(t,e)=>`mvim://open?url=file://${t}&line=${e}`,devicon:"devicon-vim-plain"},neovim:{name:"Neovim",urlTemplate:(t,e)=>`nvim://open?file=${t}&line=${e}`,devicon:"devicon-neovim-plain"},emacs:{name:"Emacs",urlTemplate:(t,e)=>`emacs://open?url=file://${t}&line=${e}`,devicon:"devicon-emacs-original"}},Ie={all:{name:"All",dotClass:"level-dot-all"},must:{name:"MUST",dotClass:"level-dot-must"},should:{name:"SHOULD",dotClass:"level-dot-should"},may:{name:"MAY",dotClass:"level-dot-may"}},Rt={rs:"devicon-rust-original",ts:"devicon-typescript-plain",tsx:"devicon-typescript-plain",js:"devicon-javascript-plain",jsx:"devicon-javascript-plain",py:"devicon-python-plain",go:"devicon-go-plain",c:"devicon-c-plain",cpp:"devicon-cplusplus-plain",h:"devicon-c-plain",hpp:"devicon-cplusplus-plain",swift:"devicon-swift-plain",java:"devicon-java-plain",rb:"devicon-ruby-plain",md:"devicon-markdown-original",json:"devicon-json-plain",yaml:"devicon-yaml-plain",yml:"devicon-yaml-plain",toml:"devicon-toml-plain",html:"devicon-html5-plain",css:"devicon-css3-plain",scss:"devicon-sass-original",sass:"devicon-sass-original",sh:"devicon-bash-plain",bash:"devicon-bash-plain",zsh:"devicon-bash-plain",sql:"devicon-postgresql-plain",kt:"devicon-kotlin-plain",scala:"devicon-scala-plain",hs:"devicon-haskell-plain",ex:"devicon-elixir-plain",exs:"devicon-elixir-plain",erl:"devicon-erlang-plain",clj:"devicon-clojure-plain",php:"devicon-php-plain",lua:"devicon-lua-plain",r:"devicon-r-plain",jl:"devicon-julia-plain",dart:"devicon-dart-plain",vue:"devicon-vuejs-plain",svelte:"devicon-svelte-plain",default:null};function Fe(t){const e=t.split(".").pop()?.toLowerCase();return Rt[e]||Rt.default}function an({filePath:t,className:e=""}){const n=Fe(t),s=V(null);return L(()=>{if(!n&&s.current&&typeof lucide<"u"){s.current.innerHTML="";const r=document.createElement("i");r.setAttribute("data-lucide","file"),s.current.appendChild(r),lucide.createIcons({nodes:[r]})}},[n]),n?v`<i class="${n} ${e}"></i>`:v`<span ref=${s} class=${e}></span>`}function ge({name:t,className:e=""}){const n=V(null);return L(()=>{if(n.current&&typeof lucide<"u"){n.current.innerHTML="";const s=document.createElement("i");s.setAttribute("data-lucide",t),n.current.appendChild(s),lucide.createIcons({nodes:[s]})}},[t]),v`<span ref=${n} class=${e}></span>`}const Me={specification:"file-text",coverage:"bar-chart-3",sources:"folder-open"};function Tr({result:t,isSelected:e,onSelect:n,onHover:s}){return v`
    <div
      class="search-modal-result ${e?"selected":""}"
      onClick=${n}
      onMouseEnter=${s}
    >
      <div class="search-modal-result-header">
        ${t.kind==="source"?v`
          <${$e} file=${t.id} line=${t.line>0?t.line:null} type="source" />
        `:v`
          <${ge} name="file-text" className="search-result-icon rule" />
          <span class="search-modal-result-id">${t.id}</span>
        `}
      </div>
      ${t.kind==="source"?v`
        <pre class="search-modal-result-code"><code dangerouslySetInnerHTML=${{__html:t.highlighted||t.content.trim()}} /></pre>
      `:v`
        <div class="search-modal-result-content" dangerouslySetInnerHTML=${{__html:t.highlighted||t.content.trim()}} />
      `}
    </div>
  `}function Cr({onClose:t,onSelect:e}){const[n,s]=M(""),[r,l]=M(null),[i,o]=M(!1),[a,u]=M(0),p=V(null),c=V(null),d=V(null);L(()=>{p.current?.focus()},[]),L(()=>{r?.results?.length&&typeof lucide<"u"&&requestAnimationFrame(()=>{lucide.createIcons()})},[r]),L(()=>{if(!n||n.length<2){l(null),u(0);return}return o(!0),d.current&&clearTimeout(d.current),d.current=setTimeout(async()=>{try{const _=await(await fetch(`/api/search?q=${encodeURIComponent(n)}&limit=50`)).json();l(_),u(0)}catch(m){console.error("Search failed:",m),l({results:[]})}finally{o(!1)}},150),()=>{d.current&&clearTimeout(d.current)}},[n]),L(()=>{if(!c.current)return;const m=c.current.querySelector(".search-modal-result.selected");m&&m.scrollIntoView({block:"nearest"})},[a]);const g=N(m=>{if(r?.results?.length){if(m.key==="ArrowDown")m.preventDefault(),u(_=>Math.min(_+1,r.results.length-1));else if(m.key==="ArrowUp")m.preventDefault(),u(_=>Math.max(_-1,0));else if(m.key==="Enter"){m.preventDefault();const _=r.results[a];_&&e(_)}}},[r,a,e]),k=N(m=>{m.target===m.currentTarget&&t()},[t]);return v`
    <div class="search-overlay" onClick=${k}>
      <div class="search-modal">
        <div class="search-modal-input">
          <input
            ref=${p}
            type="text"
            placeholder="Search code and rules..."
            value=${n}
            onInput=${m=>s(m.target.value)}
            onKeyDown=${g}
          />
        </div>
        <div class="search-modal-results" ref=${c}>
          ${i?v`
            <div class="search-modal-empty">Searching...</div>
          `:r?.results?.length>0?v`
            ${r.results.map((m,_)=>v`
              <${Tr}
                key=${m.kind+":"+m.id+":"+m.line}
                result=${m}
                isSelected=${_===a}
                onSelect=${()=>e(m)}
                onHover=${()=>u(_)}
              />
            `)}
          `:n.length>=2?v`
            <div class="search-modal-empty">No results found</div>
          `:v`
            <div class="search-modal-empty">Type to search code and rules...</div>
          `}
        </div>
        <div class="search-modal-hint">
          <span><kbd>↑</kbd><kbd>↓</kbd> Navigate</span>
          <span><kbd>Enter</kbd> Select</span>
          <span><kbd>Esc</kbd> Close</span>
        </div>
      </div>
    </div>
  `}function Lr({view:t,onViewChange:e,onOpenSearch:n}){const s=(r,l)=>{r.preventDefault(),e(l)};return v`
    <header class="header">
      <div class="header-inner">
        <nav class="nav">
          <a
            href="/spec"
            class="nav-tab ${t==="spec"?"active":""}"
            onClick=${r=>s(r,"spec")}
          ><${ge} name=${Me.specification} className="tab-icon" /><span>Specification</span></a>
          <a
            href="/coverage"
            class="nav-tab ${t==="coverage"?"active":""}"
            onClick=${r=>s(r,"coverage")}
          ><${ge} name=${Me.coverage} className="tab-icon" /><span>Coverage</span></a>
          <a
            href="/sources"
            class="nav-tab ${t==="sources"?"active":""}"
            onClick=${r=>s(r,"sources")}
          ><${ge} name=${Me.sources} className="tab-icon" /><span>Sources</span></a>
        </nav>

        <div class="search-box" style="margin-left: auto; margin-right: 1rem; display: flex; align-items: center;">
          <input
            type="text"
            class="search-input"
            placeholder="Search... (${xr}+K)"
            onClick=${n}
            onFocus=${r=>{r.target.blur(),n()}}
            readOnly
            style="cursor: pointer;"
          />
        </div>

        <a href="https://github.com/bearcove/tracey" class="logo" target="_blank" rel="noopener">tracey</a>
      </div>
    </header>
  `}function Er(t){const e=t.lastIndexOf("/");return e===-1?{dir:"",name:t}:{dir:t.slice(0,e+1),name:t.slice(e+1)}}function $e({file:t,line:e,short:n=!1,type:s="source",onClick:r,className:l=""}){const{dir:i,name:o}=Er(t),u=v`
    <${an} filePath=${t} className="file-path-icon ${s==="impl"?"file-path-icon-impl":s==="verify"?"file-path-icon-verify":""}" /><span class="file-path-text">${!n&&i?v`<span class="file-path-dir">${i}</span>`:""}<span class="file-path-name">${o}</span>${e!=null?v`<span class="file-path-line">:${e}</span>`:""}</span>
  `;return r?v`
      <a
        class="file-path-link ${l}"
        href="#"
        onClick=${p=>{p.preventDefault(),r()}}
      >
        ${u}
      </a>
    `:v`<span class="file-path-display ${l}">${u}</span>`}function Tt({file:t,line:e,type:n,onSelectFile:s}){return v`
    <div class="ref-line">
      <${$e}
        file=${t}
        line=${e}
        type=${n}
        onClick=${()=>s(t,e)}
      />
    </div>
  `}function Ar({data:t,config:e,search:n,onSearchChange:s,level:r,onLevelChange:l,filter:i,onFilterChange:o,onSelectRule:a,onSelectFile:u}){const[p,c]=M(!1);L(()=>{const h=f=>{f.target.closest("#level-dropdown")||c(!1)};return document.addEventListener("click",h),()=>document.removeEventListener("click",h)},[]);const d=q(()=>t.specs.flatMap(h=>h.rules.map(f=>({...f,spec:h.name}))),[t]),g=N(h=>{if(h.level)return h.level.toLowerCase();if(!h.text)return null;const f=h.text.toUpperCase();return f.includes("MUST")||f.includes("SHALL")||f.includes("REQUIRED")?"must":f.includes("SHOULD")||f.includes("RECOMMENDED")?"should":f.includes("MAY")||f.includes("OPTIONAL")?"may":null},[]),k=q(()=>{let h=d;if(r!=="all"&&(h=h.filter(f=>g(f)===r)),i==="impl"?h=h.filter(f=>f.implRefs.length===0):i==="verify"&&(h=h.filter(f=>f.verifyRefs.length===0)),n){const f=n.toLowerCase();h=h.filter($=>$.id.toLowerCase().includes(f)||$.text&&$.text.toLowerCase().includes(f))}return h},[d,n,r,i,g]),m=q(()=>{let h=d;r!=="all"&&(h=h.filter(b=>g(b)===r));const f=h.length,$=h.filter(b=>b.implRefs.length>0).length,x=h.filter(b=>b.verifyRefs.length>0).length;return{total:f,impl:$,verify:x,implPct:f?$/f*100:0,verifyPct:f?x/f*100:0}},[d,r,g]),_=v`<svg class="rule-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M14 3v4a1 1 0 0 0 1 1h4"/><path d="M17 21H7a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h7l5 5v11a2 2 0 0 1-2 2z"/><path d="M9 15l2 2 4-4"/></svg>`;return v`
    <div class="stats-bar">
      <div class="stat">
        <span class="stat-label">Rules</span>
        <span class="stat-value">${m.total}</span>
      </div>
      <div class="stat clickable" onClick=${()=>o(i==="impl"?null:"impl")}>
        <span class="stat-label">Impl Coverage ${i==="impl"?"(filtered)":""}</span>
        <span class="stat-value ${Be(m.implPct)}">${m.implPct.toFixed(1)}%</span>
      </div>
      <div class="stat clickable" onClick=${()=>o(i==="verify"?null:"verify")}>
        <span class="stat-label">Test Coverage ${i==="verify"?"(filtered)":""}</span>
        <span class="stat-value ${Be(m.verifyPct)}">${m.verifyPct.toFixed(1)}%</span>
      </div>

      <!-- Level dropdown -->
      <div class="custom-dropdown ${p?"open":""}" id="level-dropdown">
        <div class="dropdown-selected" onClick=${h=>{h.stopPropagation(),c(!p)}}>
          <span class="level-dot ${Ie[r].dotClass}"></span>
          <span>${Ie[r].name}</span>
          <svg class="chevron" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M6 9l6 6 6-6"/></svg>
        </div>
        <div class="dropdown-menu">
          ${Object.entries(Ie).map(([h,f])=>v`
            <div
              key=${h}
              class="dropdown-option ${r===h?"active":""}"
              onClick=${()=>{l(h),c(!1)}}
            >
              <span class="level-dot ${f.dotClass}"></span>
              <span>${f.name}</span>
            </div>
          `)}
        </div>
      </div>
    </div>
    <div class="main">
      <div class="content">
        <div class="content-body">
          <table class="rules-table">
            <thead>
              <tr>
                <th style="width: 45%">Rule</th>
                <th style="width: 55%">References</th>
              </tr>
            </thead>
            <tbody>
              ${k.map(h=>v`
                <tr key=${h.id} onClick=${()=>a(h.id)} style="cursor: pointer;">
                  <td>
                    <div class="rule-id-row">
                      ${_}
                      <span class="rule-id">${h.id}</span>
                    </div>
                    ${h.text&&v`<div class="rule-text" dangerouslySetInnerHTML=${{__html:kr(h.text)}} />`}
                  </td>
                  <td class="rule-refs" onClick=${f=>f.stopPropagation()}>
                    ${h.implRefs.length>0||h.verifyRefs.length>0?v`
                          ${h.implRefs.map(f=>v`
                            <${Tt}
                              key=${"impl:"+f.file+":"+f.line}
                              file=${f.file}
                              line=${f.line}
                              type="impl"
                              onSelectFile=${u}
                            />
                          `)}
                          ${h.verifyRefs.map(f=>v`
                            <${Tt}
                              key=${"verify:"+f.file+":"+f.line}
                              file=${f.file}
                              line=${f.line}
                              type="verify"
                              onSelectFile=${u}
                            />
                          `)}
                        `:v`<span style="color: var(--fg-dim)">—</span>`}
                  </td>
                </tr>
              `)}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  `}function Pr({data:t,forward:e,config:n,search:s,selectedFile:r,selectedLine:l,ruleContext:i,onSelectFile:o,onSelectRule:a,onClearContext:u}){const p=q(()=>mr(t.files),[t.files]),c=gr(r),d=q(()=>{if(!i||!e)return null;for(const h of e.specs){const f=h.rules.find($=>$.id===i);if(f)return f}return null},[i,e]),g={total:t.totalUnits,covered:t.coveredUnits,pct:t.totalUnits?t.coveredUnits/t.totalUnits*100:0},k=N(h=>h.file===r&&h.line===l,[r,l]),m=v`<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M18 6L6 18M6 6l12 12"/></svg>`,_=v`<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M19 12H5M12 19l-7-7 7-7"/></svg>`;return v`
    <div class="stats-bar">
      <div class="stat">
        <span class="stat-label">Code Units</span>
        <span class="stat-value">${g.total}</span>
      </div>
      <div class="stat">
        <span class="stat-label">Spec Coverage</span>
        <span class="stat-value ${Be(g.pct)}">${g.pct.toFixed(1)}%</span>
      </div>
      <div class="stat">
        <span class="stat-label">Covered</span>
        <span class="stat-value good">${g.covered}</span>
      </div>
      <div class="stat">
        <span class="stat-label">Uncovered</span>
        <span class="stat-value ${g.total-g.covered>0?"bad":"good"}">${g.total-g.covered}</span>
      </div>
    </div>
    <div class="main">
      <div class="sidebar">
        ${d?v`
          <!-- Rule context panel -->
          <div class="rule-context">
            <div class="rule-context-header">
              <span class="rule-context-id">${d.id}</span>
              <button class="rule-context-close" onClick=${u} title="Close context">
                ${m}
              </button>
            </div>
            <div class="rule-context-body">
              ${d.text&&v`
                <div class="rule-context-text">${d.text}</div>
              `}
              <div class="rule-context-refs">
                ${d.implRefs.map(h=>v`
                  <div
                    key=${"impl:"+h.file+":"+h.line}
                    class="rule-context-ref ${k(h)?"active":""}"
                    onClick=${()=>o(h.file,h.line,i)}
                    title=${h.file}
                  >
                    <${$e} file=${h.file} line=${h.line} short type="impl" />
                  </div>
                `)}
                ${d.verifyRefs.map(h=>v`
                  <div
                    key=${"verify:"+h.file+":"+h.line}
                    class="rule-context-ref ${k(h)?"active":""}"
                    onClick=${()=>o(h.file,h.line,i)}
                    title=${h.file}
                  >
                    <${$e} file=${h.file} line=${h.line} short type="verify" />
                  </div>
                `)}
              </div>
              <a class="rule-context-back" onClick=${()=>a(i)}>
                ${_}
                <span>Back to rule in spec</span>
              </a>
            </div>
          </div>
        `:v`
          <!-- Normal file tree -->
          <div class="sidebar-header">Files</div>
          <div class="sidebar-content">
            <${We}
              node=${p}
              selectedFile=${r}
              onSelectFile=${o}
              search=${s}
            />
          </div>
        `}
      </div>
      <div class="content">
        ${c?v`
          <div class="content-header">${c.path}</div>
          <div class="content-body">
            <${Ir} file=${c} config=${n} selectedLine=${l} onSelectRule=${a} />
          </div>
        `:v`
          <div class="empty-state">Select a file to view coverage</div>
        `}
      </div>
    </div>
  `}function We({node:t,selectedFile:e,onSelectFile:n,depth:s=0,search:r,parentPath:l=""}){const i=l?`${l}/${t.name}`:t.name,o=e&&e.startsWith(i+"/"),a=e&&(o||t.files.some(_=>_.path===e)),[u,p]=M(s<2||a);L(()=>{a&&!u&&p(!0)},[e,a]);const c=Object.values(t.children).sort((_,h)=>_.name.localeCompare(h.name)),d=t.files.sort((_,h)=>_.name.localeCompare(h.name)),g=_=>r?_.toLowerCase().includes(r.toLowerCase()):!0;if(s===0)return v`
      <div class="file-tree">
        ${c.map(_=>v`
          <${We}
            key=${_.name}
            node=${_}
            selectedFile=${e}
            onSelectFile=${n}
            depth=${s+1}
            search=${r}
            parentPath=""
          />
        `)}
        ${d.filter(_=>g(_.path)).map(_=>v`
          <${Ct}
            key=${_.path}
            file=${_}
            selected=${e===_.path}
            onClick=${()=>n(_.path)}
          />
        `)}
      </div>
    `;const k=d.some(_=>g(_.path))||c.some(_=>Object.values(_.children).length>0||_.files.some(h=>g(h.path)));if(r&&!k)return null;const m=ln(t.coveredUnits,t.totalUnits);return v`
    <div class="tree-folder ${u?"open":""}">
      <div class="tree-folder-header" onClick=${()=>p(!u)}>
        <div class="tree-folder-left">
          <svg class="tree-folder-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M9 18l6-6-6-6"/>
          </svg>
          <span>${t.name}</span>
        </div>
        <span class="folder-badge ${m.class}">${m.text}</span>
      </div>
      <div class="tree-folder-children">
        ${c.map(_=>v`
          <${We}
            key=${_.name}
            node=${_}
            selectedFile=${e}
            onSelectFile=${n}
            depth=${s+1}
            search=${r}
            parentPath=${i}
          />
        `)}
        ${d.filter(_=>g(_.path)).map(_=>v`
          <${Ct}
            key=${_.path}
            file=${_}
            selected=${e===_.path}
            onClick=${()=>n(_.path)}
          />
        `)}
      </div>
    </div>
  `}function Ct({file:t,selected:e,onClick:n}){const s=ln(t.coveredUnits,t.totalUnits);return v`
    <div
      class="tree-file ${e?"selected":""}"
      onClick=${n}
    >
      <${an} filePath=${t.name} className="tree-file-icon" />
      <span class="tree-file-name">${t.name}</span>
      <span class="tree-file-badge ${s.class}">${s.text}</span>
    </div>
  `}function Ir({file:t,config:e,selectedLine:n,onSelectRule:s}){const r=t.content.split(`
`),l=br(t.content,"rust"),[i,o]=M(null),[a,u]=M(null),p=V(null),c=q(()=>{const k=new Map;for(const m of t.units)for(let _=m.startLine;_<=m.endLine;_++){k.has(_)||k.set(_,{units:[],ruleRefs:new Set});const h=k.get(_);h.units.push(m);for(const f of m.ruleRefs)h.ruleRefs.add(f)}return k},[t]),d=l||r.map(k=>k.replace(/&/g,"&amp;").replace(/</g,"&lt;").replace(/>/g,"&gt;")),g=e?.projectRoot?`${e.projectRoot}/${t.path}`:t.path;return L(()=>{n&&p.current&&d&&requestAnimationFrame(()=>{const k=p.current?.querySelector(`[data-line="${n}"]`);if(k){const m=p.current.closest(".content-body");if(m){const _=k.offsetHeight,f=k.offsetTop-_*5-120;m.scrollTo({top:Math.max(0,f)})}u(n)}})},[n,t.path,d]),L(()=>{const k=m=>{!m.target.closest(".line-popover")&&!m.target.closest(".line-number")&&o(null)};return document.addEventListener("click",k),()=>document.removeEventListener("click",k)},[]),v`
    <div class="code-view" ref=${p}>
      ${d.map((k,m)=>{const _=m+1,h=c.get(_),f=h&&h.ruleRefs.size>0,$=h&&h.units.length>0;return v`
          <div
            key=${_}
            data-line=${_}
            class="code-line ${$?f?"covered":"uncovered":""} ${a===_?"highlighted":""}"
          >
            <span
              class="line-number"
              onClick=${b=>{b.stopPropagation(),o(i===_?null:_)}}
            >
              ${_}
              ${i===_&&v`
                <div class="line-popover">
                  ${Object.entries(Rr).map(([b,w])=>v`
                    <a
                      key=${b}
                      href=${w.urlTemplate(g,_)}
                      class="popover-btn"
                      title="Open in ${w.name}"
                    >
                      ${w.devicon?v`<i class="${w.devicon}"></i>`:v`<span dangerouslySetInnerHTML=${{__html:w.icon}}></span>`}
                      <span>${w.name}</span>
                    </a>
                  `)}
                </div>
              `}
            </span>
            <span
              class="line-content"
              dangerouslySetInnerHTML=${{__html:k||" "}}
            />
            ${h&&h.ruleRefs.size>0&&v`
              <span class="line-annotations">
                <span class="annotation-count" title=${[...h.ruleRefs].join(", ")}>${h.ruleRefs.size}</span>
                <span class="annotation-badges">
                  ${[...h.ruleRefs].map(b=>v`
                    <a
                      key=${b}
                      class="annotation-badge"
                      href=${sn("spec",{rule:b})}
                      onClick=${w=>{w.preventDefault(),s(b)}}
                    >${b}</a>
                  `)}
                </span>
              </span>
            `}
          </div>
        `})}
    </div>
  `}function Mr({config:t,forward:e,selectedRule:n,onSelectRule:s,onSelectFile:r,scrollPosition:l,onScrollChange:i}){const o=_r(t.specs[0]?.name),[a,u]=M(null),p=V(null),c=V(null),d=V(l),g=q(()=>{const h=new Map;for(const f of e.specs)for(const $ of f.rules){const x=$.implRefs.length>0,b=$.verifyRefs.length>0;h.set($.id,{rule:$,status:x&&b?"covered":x||b?"partial":"uncovered"})}return h},[e]),k=q(()=>{if(!o)return[];const h=[],f=o.content.split(`
`);for(const $ of f){const x=$.match(/^(#{1,4})\s+(.+)$/);if(x){const b=x[1].length,w=x[2].trim(),I=w.toLowerCase().replace(/[^\w]+/g,"-").replace(/^-|-$/g,"");h.push({level:b,text:w,slug:I})}}return h},[o]),m=q(()=>{if(!o)return"";let h=o.content;h=h.replace(/(^|\n\n)r\[([^\]]+)\]/g,($,x,b)=>{const w=g.get(b),I=w?.status||"uncovered",H=w?.rule,U=C=>C.split("/").pop();let O="";if(H){const C=[];H.implRefs&&H.implRefs.length>0&&H.implRefs.forEach(E=>{const Y=Fe(E.file),Re=Y?`<i class="${Y} spec-ref-icon"></i>`:'<i data-lucide="file" class="spec-ref-icon"></i>';C.push(`<a class="spec-ref spec-ref-impl" href="/tree/${E.file}:${E.line}" data-file="${E.file}" data-line="${E.line}" title="${E.file}:${E.line}">${Re}${U(E.file)}:${E.line}</a>`)}),H.verifyRefs&&H.verifyRefs.length>0&&H.verifyRefs.forEach(E=>{const Y=Fe(E.file),Re=Y?`<i class="${Y} spec-ref-icon"></i>`:'<i data-lucide="file" class="spec-ref-icon"></i>';C.push(`<a class="spec-ref spec-ref-verify" href="/tree/${E.file}:${E.line}" data-file="${E.file}" data-line="${E.line}" title="${E.file}:${E.line}">${Re}${U(E.file)}:${E.line}</a>`)}),C.length>0&&(O=C.join(""))}return`${x}<!--RULE_START:${b}:${I}--><a class="rule-marker ${I}" href="/spec/${b}" data-rule="${b}"><i data-lucide="file-check" class="rule-marker-icon"></i>${b}</a>${O?`<div class="spec-refs">${O}</div>`:""}<!--RULE_CONTENT_START-->`});let f=R.parse(h);return k.forEach($=>{const x=new RegExp(`(<h${$.level}>)(${$.text.replace(/[.*+?^${}()|[\]\\]/g,"\\$&")})(</h${$.level}>)`,"i");f=f.replace(x,`<h${$.level} id="${$.slug}" data-slug="${$.slug}">$2$3`)}),f=f.replace(/<!--RULE_START:([^:]+):([^-]+)-->([\s\S]*?)<!--RULE_CONTENT_START-->([\s\S]*?)(?=<!--RULE_START|<h[1-6]|$)/g,($,x,b,w,I)=>`<div class="rule-block rule-block-${b}"><div class="rule-block-header">${w}</div><div class="rule-block-content">${I.trim()}</div></div>`),f},[o,g,k]);L(()=>{if(!p.current||!c.current||k.length===0)return;const h=setTimeout(()=>{const f=p.current.querySelectorAll("h1[id], h2[id], h3[id], h4[id]");if(f.length===0)return;const $=new IntersectionObserver(x=>{const b=[];x.forEach(w=>{w.isIntersecting&&b.push({id:w.target.id,top:w.boundingClientRect.top})}),b.length>0&&(b.sort((w,I)=>w.top-I.top),u(b[0].id))},{root:c.current,rootMargin:"-5% 0px -70% 0px",threshold:0});return f.forEach(x=>$.observe(x)),k.length>0&&u(k[0].slug),()=>$.disconnect()},100);return()=>clearTimeout(h)},[m,k]),L(()=>{if(!c.current)return;const h=()=>{i&&i(c.current.scrollTop)};return c.current.addEventListener("scroll",h,{passive:!0}),()=>c.current?.removeEventListener("scroll",h)},[i]),L(()=>{m&&p.current&&typeof lucide<"u"&&requestAnimationFrame(()=>{lucide.createIcons({nodes:p.current.querySelectorAll("[data-lucide]")})})},[m]);const _=N(h=>{if(!p.current||!c.current)return;const f=p.current.querySelector(`[id="${h}"]`);if(f){const $=f.offsetTop-100;c.current.scrollTo({top:Math.max(0,$)}),u(h)}},[]);return L(()=>{if(!p.current)return;const h=f=>{const $=f.target.closest("h1[id], h2[id], h3[id], h4[id]");if($){const w=$.id,I=`${window.location.origin}${window.location.pathname}#${w}`;navigator.clipboard?.writeText(I);return}const x=f.target.closest("a.rule-marker[data-rule]");if(x){f.preventDefault();const w=x.dataset.rule;s(w);return}const b=f.target.closest("a.spec-ref");if(b){f.preventDefault();const w=b.dataset.file,I=parseInt(b.dataset.line,10),O=b.closest(".rule-block")?.querySelector("a.rule-marker[data-rule]")?.dataset.rule||null;r(w,I,O);return}};return p.current.addEventListener("click",h),()=>p.current?.removeEventListener("click",h)},[m,s,r]),L(()=>{if(!m)return;let h=!1;return requestAnimationFrame(()=>{h||requestAnimationFrame(()=>{if(!(h||!p.current||!c.current))if(n){const f=p.current.querySelector(`[data-rule="${n}"]`);if(f){const $=c.current.getBoundingClientRect(),x=f.getBoundingClientRect(),w=c.current.scrollTop+(x.top-$.top)-150;c.current.scrollTo({top:Math.max(0,w)}),f.classList.add("rule-marker-highlighted"),setTimeout(()=>{f.classList.remove("rule-marker-highlighted")},3e3)}}else d.current>0&&(c.current.scrollTo({top:d.current}),d.current=0)})}),()=>{h=!0}},[n,m]),o?v`
    <div class="main">
      <div class="sidebar">
        <div class="sidebar-header">Outline</div>
        <div class="sidebar-content">
          <div class="outline-tree">
            ${k.map(h=>v`
              <div
                key=${h.slug}
                class="outline-item outline-level-${h.level} ${a===h.slug?"active":""}"
                onClick=${()=>_(h.slug)}
              >
                ${h.text}
              </div>
            `)}
          </div>
        </div>
      </div>
      <div class="content">
        <div class="content-header">
          ${o.sourceFile||o.name}
        </div>
        <div class="content-body" ref=${c}>
          <div
            class="markdown"
            ref=${p}
            dangerouslySetInnerHTML=${{__html:m}}
          />
        </div>
      </div>
    </div>
  `:v`
      <div class="main">
        <div class="empty-state">Loading spec...</div>
      </div>
    `}dn(v`<${yr} />`,document.getElementById("app"));document.addEventListener("keydown",t=>{(t.metaKey||t.ctrlKey)&&t.key==="k"&&(t.preventDefault(),document.querySelector(".search-input")?.focus())});
