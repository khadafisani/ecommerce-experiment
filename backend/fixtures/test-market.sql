-- Account numbers are stable so people can share the same testing scenarios.
INSERT INTO users(email,name,password,admin,created_at,updated_at)
VALUES ('admin@seed.tosca.invalid','Fixture Administrator','!',TRUE,@seed_now,@seed_now);
SET @seed_admin = LAST_INSERT_ID();
INSERT INTO user_credentials(user_id,password_hash) VALUES (@seed_admin,@seed_password);

CREATE TEMPORARY TABLE fixture_buyer (
    n INT PRIMARY KEY, email VARCHAR(254), display_name VARCHAR(120), city VARCHAR(60)
) ENGINE=InnoDB;
INSERT INTO fixture_buyer
SELECT seq, CONCAT('buyer',LPAD(seq,3,'0'),'@seed.tosca.invalid'),
       CONCAT(ELT(1+MOD(seq-1,12),'Ayu','Bagas','Citra','Dimas','Eka','Fajar','Gita','Hana','Indra','Jihan','Kirana','Laras'),
              ' ',ELT(1+MOD(FLOOR((seq-1)/12),20),'Pratama','Lestari','Wijaya','Putri','Santoso','Utami','Saputra','Nugraha','Maulana','Rahma','Wibowo','Ananda','Nabila','Ramadhan','Hidayat','Kusuma','Permata','Akbar','Iskandar','Sari')),
       ELT(1+MOD(seq-1,6),'Jakarta','Bandung','Surabaya','Yogyakarta','Denpasar','Semarang')
FROM seq_1_to_240;

INSERT INTO users(email,name,password,suspended,created_at,updated_at)
SELECT email,display_name,'!',n=240,@seed_now-INTERVAL 1460 DAY,@seed_now FROM fixture_buyer;
INSERT INTO user_credentials(user_id,password_hash)
SELECT u.id,@seed_password FROM users u JOIN fixture_buyer f ON f.email=u.email;

CREATE TEMPORARY TABLE fixture_shop (
    n INT PRIMARY KEY, short_name VARCHAR(120), owner_email VARCHAR(254)
) ENGINE=InnoDB;
INSERT INTO fixture_shop
SELECT seq, CONCAT(ELT(1+MOD(seq-1,8),'Pagi','Ruang','Awan','Laras','Taman','Rimba','Senja','Bumi'),
                   ' ',ELT(1+FLOOR((seq-1)/8),'Studio','Supply','Goods')),
       CONCAT('owner',LPAD(seq,2,'0'),'@seed.tosca.invalid')
FROM seq_1_to_24;

INSERT INTO users(email,name,password,created_at,updated_at)
SELECT owner_email,CONCAT(short_name,' Owner'),'!',@seed_now-INTERVAL 1460 DAY,@seed_now FROM fixture_shop;
INSERT INTO user_credentials(user_id,password_hash)
SELECT u.id,@seed_password FROM users u JOIN fixture_shop f ON f.owner_email=u.email;
INSERT INTO users(email,name,password,created_at,updated_at)
SELECT CONCAT('staff',LPAD(s.n,2,'0'),'-',r.seq,'@seed.tosca.invalid'),
       CONCAT(s.short_name,ELT(r.seq,' Fulfillment',' Catalog')),
       '!',@seed_now-INTERVAL 1460 DAY,@seed_now
FROM fixture_shop s CROSS JOIN seq_1_to_2 r;
INSERT INTO user_credentials(user_id,password_hash)
SELECT u.id,@seed_password FROM fixture_shop s CROSS JOIN seq_1_to_2 r
JOIN users u ON u.email=CONCAT('staff',LPAD(s.n,2,'0'),'-',r.seq,'@seed.tosca.invalid');

INSERT INTO shops(name,owner_id,suspended,created_at,updated_at)
SELECT fs.short_name,u.id,fs.n=24,@seed_now-INTERVAL 1460 DAY,@seed_now
FROM fixture_shop fs JOIN users u ON u.email=fs.owner_email;
CREATE TEMPORARY TABLE fixture_shop_id (
    n INT PRIMARY KEY, shop_id BIGINT NOT NULL, owner_id BIGINT NOT NULL, staff_id BIGINT NOT NULL
) ENGINE=InnoDB;
INSERT INTO fixture_shop_id
SELECT fs.n,s.id,s.owner_id,staff.id
FROM fixture_shop fs JOIN users u ON u.email=fs.owner_email
JOIN shops s ON s.owner_id=u.id
JOIN users staff ON staff.email=CONCAT('staff',LPAD(fs.n,2,'0'),'-1@seed.tosca.invalid');

INSERT INTO members(shop_id,user_id,created_at,updated_at)
SELECT shop_id,owner_id,@seed_now-INTERVAL 1460 DAY,@seed_now FROM fixture_shop_id;
INSERT INTO members(shop_id,user_id,created_at,updated_at)
SELECT fs.shop_id,u.id,@seed_now-INTERVAL 1460 DAY,@seed_now
FROM fixture_shop_id fs CROSS JOIN seq_1_to_2 r
JOIN users u ON u.email=CONCAT('staff',LPAD(fs.n,2,'0'),'-',r.seq,'@seed.tosca.invalid');

CREATE TEMPORARY TABLE fixture_template (
    n INT PRIMARY KEY, category_name VARCHAR(120), product_name VARCHAR(180), image VARCHAR(30), base_price INT, grams INT, description VARCHAR(255)
) ENGINE=InnoDB;
INSERT INTO fixture_template VALUES
    (1,'Fashion','Everyday Sneakers','sneakers',289000,740,'Low-top sneakers for everyday wear.'),
    (2,'Home & Living','Stoneware Mug','mug',79000,360,'A glazed stoneware mug with a comfortable handle.'),
    (3,'Electronics','Wireless Headphones','headphones',419000,280,'Wireless over-ear headphones with foldable earcups.'),
    (4,'Fashion','Canvas Carryall','bag',139000,320,'A roomy bag for books, groceries, and daily essentials.'),
    (5,'Home & Living','Desk Lamp','lamp',249000,920,'A compact desk lamp with an adjustable shade.'),
    (6,'Beauty','Botanical Hair Mask','beauty',119000,220,'A conditioning mask in a travel-friendly tube.'),
    (7,'Sports','Training Shoes','running',379000,680,'Light training shoes with a grippy rubber sole.'),
    (8,'Food & Drink','Ground Coffee','coffee',69000,250,'A small-batch roast ground for filter brewing.');

INSERT INTO taxonomy(kind,name,status,created_at,updated_at)
SELECT DISTINCT 'category',category_name,'approved',@seed_now,@seed_now FROM fixture_template
ON DUPLICATE KEY UPDATE id=id;
INSERT INTO taxonomy(kind,name,status,created_at,updated_at)
SELECT 'brand',CONCAT(short_name,' Series'),'approved',@seed_now,@seed_now FROM fixture_shop
ON DUPLICATE KEY UPDATE id=id;
INSERT INTO taxonomy(kind,name,status,created_at,updated_at)
SELECT kinds.kind,CONCAT('Fixture ',kinds.label,' ',LPAD(numbers.seq,2,'0')),
       IF(numbers.seq<=24,'pending','rejected'),@seed_now-INTERVAL numbers.seq DAY,@seed_now
FROM seq_1_to_28 numbers
CROSS JOIN (SELECT 'brand' AS kind,'Brand' AS label UNION ALL SELECT 'category','Category') kinds
WHERE TRUE
ON DUPLICATE KEY UPDATE id=id;

CREATE TEMPORARY TABLE fixture_product (
    shop_no INT, product_no INT, name VARCHAR(180), base_price INT, grams INT, PRIMARY KEY(shop_no,product_no)
) ENGINE=InnoDB;
INSERT INTO fixture_product
SELECT f.n,k.seq,
       CONCAT(t.product_name,' ',ELT(1+FLOOR((k.seq-1)/8),'Classic','Select','Signature','Limited'),' - ',f.short_name),
       t.base_price+2000*MOD(f.n*k.seq,51),t.grams
FROM fixture_shop f CROSS JOIN seq_1_to_32 k
JOIN fixture_template t ON t.n=1+MOD(k.seq-1,8);

INSERT INTO products(shop_id,name,description,image,brand_id,category_id,status,note,actor_id,created_at,updated_at)
SELECT s.shop_id,p.name,CONCAT(t.description,' From the ',f.short_name,' collection.'),CONCAT('/catalog/',t.image,'.jpg'),
       IF(p.product_no=25,pending_brand.id,b.id),IF(p.product_no=26,pending_category.id,c.id),
       CASE WHEN p.product_no<=24 THEN 'active' WHEN p.product_no<=28 THEN 'pending'
            WHEN p.product_no<=30 THEN 'inactive' ELSE 'rejected' END,
       CASE WHEN p.product_no IN (29,30) THEN 'Listing paused for inventory reconciliation.'
            WHEN p.product_no>=31 THEN 'Please clarify the product description and resubmit.' ELSE '' END,
       IF(MOD(p.product_no,3)=0,s.staff_id,s.owner_id),
       @seed_now-INTERVAL (32-p.product_no) DAY,@seed_now
FROM fixture_product p JOIN fixture_shop f ON f.n=p.shop_no
JOIN fixture_shop_id s ON s.n=p.shop_no JOIN fixture_template t ON t.n=1+MOD(p.product_no-1,8)
JOIN taxonomy b ON b.kind='brand' AND b.name=CONCAT(f.short_name,' Series')
JOIN taxonomy c ON c.kind='category' AND c.name=t.category_name
JOIN taxonomy pending_brand ON pending_brand.kind='brand' AND pending_brand.name=CONCAT('Fixture Brand ',LPAD(f.n,2,'0'))
JOIN taxonomy pending_category ON pending_category.kind='category' AND pending_category.name=CONCAT('Fixture Category ',LPAD(f.n,2,'0'))
ORDER BY p.product_no,p.shop_no;

CREATE TEMPORARY TABLE fixture_product_id (
    shop_no INT, product_no INT, product_id BIGINT NOT NULL, PRIMARY KEY(shop_no,product_no)
) ENGINE=InnoDB;
INSERT INTO fixture_product_id
SELECT f.shop_no,f.product_no,p.id
FROM fixture_product f JOIN fixture_shop_id s ON s.n=f.shop_no
JOIN products p ON p.shop_id=s.shop_id AND p.name=f.name;

INSERT INTO variants(product_id,name,subvariant,price,stock,grams,enabled,actor_id,created_at,updated_at)
SELECT pi.product_id,ELT(v.seq,'Standard','Premium','Bundle'),ELT(v.seq,'Original','Limited color','Gift pack'),
       f.base_price+(v.seq-1)*17000,
       CASE WHEN MOD(f.product_no,8)=0 THEN 0 WHEN MOD(f.product_no,7)=0 THEN IF(v.seq=1,1,0) ELSE 35+MOD(f.shop_no*f.product_no*v.seq,193) END,
       f.grams+(v.seq-1)*120,v.seq<>3 OR MOD(f.product_no,4)<>0,s.staff_id,@seed_now-INTERVAL 32 DAY,@seed_now
FROM fixture_product f JOIN fixture_product_id pi USING (shop_no,product_no)
JOIN fixture_shop_id s ON s.n=f.shop_no CROSS JOIN seq_1_to_3 v;

INSERT INTO addresses(user_id,label,detail,created_at,updated_at)
SELECT u.id,ELT(a.seq,'Home','Office'),
       CONCAT(b.display_name,', Fixture ',ELT(a.seq,'Residence','Office'),' Unit ',b.n,', ',b.city,', Indonesia (test address)'),
       @seed_now-INTERVAL 1 YEAR,@seed_now
FROM fixture_buyer b JOIN users u ON u.email=b.email CROSS JOIN seq_1_to_2 a;

INSERT INTO carts(user_id,variant_id,quantity,created_at,updated_at)
SELECT u.id,v.id,1+MOD(sample.seq,3),
       @seed_now-INTERVAL ELT(sample.seq,1,8,40,400) DAY,@seed_now
FROM fixture_buyer b JOIN users u ON u.email=b.email CROSS JOIN seq_1_to_4 sample
JOIN fixture_product_id p ON p.shop_no=1+MOD(b.n-1,23)
     AND p.product_no=ELT(sample.seq,1,8,29,2)
JOIN variants v ON v.product_id=p.product_id AND v.name=IF(sample.seq=4,'Bundle','Standard');

INSERT INTO couriers(name,created_at,updated_at)
VALUES ('Fixture Parcel',@seed_now,@seed_now),('Fixture Express',@seed_now,@seed_now)
ON DUPLICATE KEY UPDATE id=id;

SET @seed_courier = (SELECT id FROM couriers WHERE name='Fixture Parcel');

CREATE TEMPORARY TABLE fixture_checkout (
    n INT PRIMARY KEY, checkout_no CHAR(36) NOT NULL UNIQUE, buyer BIGINT NOT NULL, address VARCHAR(1100) NOT NULL,
    scenario INT NOT NULL, created DATETIME NOT NULL, historical BOOLEAN NOT NULL
) ENGINE=InnoDB;
INSERT INTO fixture_checkout
SELECT x.seq,UUID(),u.id,CONCAT(a.label,': ',a.detail),MOD(x.seq-1,12),
       @seed_now-INTERVAL (CASE WHEN x.seq<=1650 THEN MOD(x.seq*13,1455)+4
                              WHEN MOD(x.seq-1,12)>=8 THEN 8 ELSE MOD(x.seq,10)+2 END) DAY,
       x.seq<=1650
FROM seq_1_to_1800 x JOIN fixture_buyer b ON b.n=IF(x.seq>1680,1,1+MOD(x.seq-1,239))
JOIN users u ON u.email=b.email JOIN addresses a ON a.user_id=u.id AND a.label='Home';

CREATE TEMPORARY TABLE fixture_order (
    n INT, position_no INT, shop_no INT NOT NULL, status VARCHAR(20) NOT NULL,
    PRIMARY KEY(n,position_no)
) ENGINE=InnoDB;
INSERT INTO fixture_order
SELECT c.n,g.seq,
       CASE WHEN c.n>1680 THEN IF(g.seq=1,1,2+MOD(c.n+5,22)) WHEN g.seq=1 THEN 1+MOD(c.n-1,23) ELSE 1+MOD(c.n+5,23) END,
       CASE WHEN c.historical THEN 'completed'
            WHEN c.scenario=0 THEN 'waiting_payment'
            WHEN c.scenario=1 OR (c.scenario=2 AND g.seq=2) THEN 'waiting_shop'
            WHEN c.scenario IN (2,3) AND g.seq=1 THEN 'packing'
            WHEN c.scenario=3 AND g.seq=2 THEN 'rejected'
            WHEN c.scenario=4 THEN IF(g.seq=1,'delivery','packing')
            WHEN c.scenario=5 THEN 'delivery'
            WHEN c.scenario=7 THEN 'cancelled'
            WHEN g.seq=1 AND c.scenario>=8 THEN ELT(c.scenario-7,'return_review','return_accepted','return_rejected','return_completed')
            ELSE 'completed' END
FROM fixture_checkout c JOIN seq_1_to_2 g ON g.seq=1 OR (c.historical AND MOD(c.n,3)<>0)
    OR (NOT c.historical AND c.scenario NOT IN (0,5,6,11));

CREATE TEMPORARY TABLE fixture_line (
    n INT, position_no INT, line_no INT, product_id BIGINT NOT NULL, variant_id BIGINT NOT NULL,
    name VARCHAR(180) NOT NULL, variant_name VARCHAR(306) NOT NULL, price BIGINT NOT NULL,
    quantity INT NOT NULL, grams INT NOT NULL, PRIMARY KEY(n,position_no,line_no)
) ENGINE=InnoDB;
INSERT INTO fixture_line
SELECT o.n,o.position_no,l.seq,p.id,v.id,p.name,CONCAT_WS(' / ',v.name,v.subvariant),
       v.price,1+MOD(o.n+l.seq,3),v.grams
FROM fixture_order o CROSS JOIN seq_1_to_2 l
JOIN fixture_product_id pi ON pi.shop_no=o.shop_no AND pi.product_no=1+MOD(o.n+l.seq*9+o.position_no,24)
JOIN products p ON p.id=pi.product_id
JOIN variants v ON v.product_id=p.id AND v.name=IF(MOD(o.n+l.seq,3)=0,'Premium','Standard');

CREATE TEMPORARY TABLE fixture_total (
    n INT, position_no INT, subtotal BIGINT NOT NULL, shipping BIGINT NOT NULL, tax BIGINT NOT NULL,
    PRIMARY KEY(n,position_no)
) ENGINE=InnoDB;
INSERT INTO fixture_total
SELECT n,position_no,SUM(price*quantity),SUM(grams*quantity),
       FLOOR((SUM(price*quantity)*@seed_tax+5000)/10000)
FROM fixture_line GROUP BY n,position_no;

INSERT INTO checkouts(checkout_no,user_id,address,subtotal,tax,shipping,total,payment,paid_by,created_at,updated_at,stock_reserved)
SELECT c.checkout_no,c.buyer,c.address,sum_order.subtotal,sum_order.tax,sum_order.shipping,
       sum_order.subtotal+sum_order.tax+sum_order.shipping,
       CASE WHEN c.historical THEN 'paid' WHEN c.scenario=0 THEN 'waiting' WHEN c.scenario=7 THEN 'cancelled' ELSE 'paid' END,
       IF(c.historical OR c.scenario NOT IN (0,7),@seed_admin,NULL),c.created,c.created,FALSE
FROM fixture_checkout c JOIN (
    SELECT n,SUM(subtotal) subtotal,SUM(shipping) shipping,SUM(tax) tax FROM fixture_total GROUP BY n
) sum_order ON sum_order.n=c.n
ORDER BY c.created,c.n;

INSERT INTO orders(checkout_id,shop_id,shipping,subtotal,tax,status,courier_id,receipt,completed_at,actor_id,created_at,updated_at)
SELECT db.id,s.shop_id,total.shipping,total.subtotal,total.tax,o.status,
       CASE WHEN o.status IN ('delivery','completed','return_review','return_accepted','return_rejected','return_completed')
            THEN IF(MOD(o.n,3)=0,@seed_courier,1) ELSE NULL END,
       CASE WHEN o.status IN ('delivery','completed','return_review','return_accepted','return_rejected','return_completed')
            THEN CONCAT('FIX-',o.n,'-',o.position_no) ELSE '' END,
       IF(o.status IN ('completed','return_review','return_accepted','return_rejected','return_completed'),
          f.created+INTERVAL 2 DAY,NULL),
       CASE WHEN o.status IN ('waiting_payment','cancelled','completed','return_review') THEN db.user_id
            WHEN o.status='waiting_shop' THEN @seed_admin ELSE s.staff_id END,
       f.created,LEAST(@seed_now,f.created+INTERVAL 2 DAY)
FROM fixture_order o JOIN fixture_checkout f ON f.n=o.n
JOIN checkouts db ON db.checkout_no=f.checkout_no JOIN fixture_shop_id s ON s.n=o.shop_no
JOIN fixture_total total ON total.n=o.n AND total.position_no=o.position_no
ORDER BY f.created,o.n,o.position_no;

CREATE TEMPORARY TABLE fixture_order_id (
    n INT, position_no INT, order_id BIGINT NOT NULL, PRIMARY KEY(n,position_no)
) ENGINE=InnoDB;
INSERT INTO fixture_order_id
SELECT o.n,o.position_no,db.id FROM fixture_order o JOIN fixture_checkout f ON f.n=o.n
JOIN checkouts c ON c.checkout_no=f.checkout_no JOIN fixture_shop_id s ON s.n=o.shop_no
JOIN orders db ON db.checkout_id=c.id AND db.shop_id=s.shop_id;

INSERT INTO order_lines(order_id,variant_id,product_id,product_name,variant_name,price,quantity,grams,created_at,updated_at)
SELECT oi.order_id,l.variant_id,l.product_id,l.name,l.variant_name,l.price,l.quantity,l.grams,c.created,c.created
FROM fixture_line l JOIN fixture_order_id oi USING (n,position_no) JOIN fixture_checkout c ON c.n=l.n;

INSERT INTO tracking(order_id,note,actor_id,created_at,updated_at)
SELECT oi.order_id,ELT(t.seq,'Order dispatched','Parcel at local distribution hub','Courier out for delivery'),
       s.staff_id,c.created+INTERVAL t.seq*6 HOUR,c.created+INTERVAL t.seq*6 HOUR
FROM fixture_order o JOIN fixture_order_id oi USING (n,position_no)
JOIN fixture_checkout c ON c.n=o.n JOIN fixture_shop_id s ON s.n=o.shop_no
JOIN orders db ON db.id=oi.order_id
JOIN seq_1_to_3 t ON t.seq=1 OR db.courier_id=1
WHERE db.courier_id IS NOT NULL;

INSERT INTO returns(order_id,description,media,created_at,updated_at)
SELECT o.id,ELT(1+MOD(f.n,3),'The item delivered was a different variant.','The package arrived with visible damage.',
                                   'One part was missing from the package.'),
       JSON_ARRAY(CONCAT('/api/return-media/', f.buyer, '-d3e00000-0000-4000-8000-',LPAD(LOWER(HEX(o.id)),12,'0'),'.avif')),
       o.completed_at+INTERVAL 1 DAY,o.completed_at+INTERVAL 1 DAY
FROM fixture_checkout f JOIN fixture_order_id oi ON oi.n=f.n JOIN orders o ON o.id=oi.order_id
WHERE o.status IN ('return_review','return_accepted','return_rejected','return_completed');

INSERT INTO reviews(order_id,product_id,user_id,rating,body,media,reply,actor_id,deleted,created_at,updated_at)
SELECT o.id,l.product_id,c.buyer,
       ELT(1+MOD(c.n+l.line_no,8),5,5,4,4,3,5,2,1),
       ELT(1+MOD(c.n+l.line_no,6),'Good quality and arrived on time.','Exactly as described.',
           'The packaging could be better.','Happy with the purchase.','Works well for everyday use.',
           'The finish is a bit different from the photos.'),
       JSON_ARRAY(),IF(MOD(c.n,3)=0,'Thanks for sharing your experience.',NULL),
       IF(MOD(c.n,3)=0,s.staff_id,c.buyer),MOD(c.n,29)=0,
       o.completed_at+INTERVAL 12 HOUR,o.completed_at+INTERVAL 12 HOUR
FROM fixture_checkout c JOIN fixture_order_id oi ON oi.n=c.n JOIN orders o ON o.id=oi.order_id
JOIN fixture_order fo ON fo.n=oi.n AND fo.position_no=oi.position_no JOIN fixture_shop_id s ON s.n=fo.shop_no
JOIN fixture_line l ON l.n=c.n AND l.position_no=oi.position_no
WHERE o.completed_at IS NOT NULL AND MOD(c.n+l.line_no,3)=0;

INSERT INTO notifications(user_id,message,read_at,created_at,updated_at)
SELECT c.buyer,CONCAT('Checkout ',c.checkout_no,' is ',ELT(1+MOD(c.n,3),'ready for payment','being processed','updated'),'.'),
       IF(MOD(c.n,2)=0,@seed_now,NULL),c.created,c.created
FROM fixture_checkout c WHERE NOT c.historical;

-- Revoked records populate unique-user charts without issuing usable sessions.
INSERT INTO sessions(id,user_id,refresh_hash,expires_at,revoked,created_at,updated_at)
SELECT UUID(),u.id,REPEAT('0',64),@seed_now-INTERVAL 1 DAY,TRUE,
       @seed_now-INTERVAL (CASE WHEN s.seq<=240 THEN MOD(s.seq*13,1460) ELSE MOD(s.seq*7,60) END) DAY,@seed_now
FROM seq_1_to_1800 s JOIN fixture_buyer b ON b.n=1+MOD(s.seq*17,239)
JOIN users u ON u.email=b.email;

DROP TEMPORARY TABLE fixture_total, fixture_line, fixture_order, fixture_checkout, fixture_product_id, fixture_product, fixture_template, fixture_shop_id, fixture_shop, fixture_buyer;
SET @seed_password = NULL;
