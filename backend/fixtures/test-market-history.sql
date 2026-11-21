SET @fixture_admin = (SELECT id FROM users WHERE email='admin@seed.tosca.invalid');

CREATE TEMPORARY TABLE history_buyers (
    slot INT PRIMARY KEY, user_id BIGINT NOT NULL, address VARCHAR(1100) NOT NULL
) ENGINE=InnoDB;
INSERT INTO history_buyers
SELECT ROW_NUMBER() OVER (ORDER BY u.email), u.id, CONCAT(a.label,': ',a.detail)
FROM users u
JOIN addresses a ON a.id=(SELECT MIN(id) FROM addresses WHERE user_id=u.id)
WHERE u.email REGEXP '^buyer[0-9]{3}@seed[.]tosca[.]invalid$' AND NOT u.suspended;

CREATE TEMPORARY TABLE history_variants (
    shop_id BIGINT NOT NULL, slot INT NOT NULL, slots INT NOT NULL, variant_id BIGINT NOT NULL,
    product_id BIGINT NOT NULL, product_name VARCHAR(180) NOT NULL, variant_name VARCHAR(306) NOT NULL,
    price BIGINT NOT NULL, grams INT NOT NULL, PRIMARY KEY (shop_id,slot)
) ENGINE=InnoDB;
INSERT INTO history_variants
SELECT s.id,ROW_NUMBER() OVER (PARTITION BY s.id ORDER BY v.id),
       COUNT(*) OVER (PARTITION BY s.id),v.id,p.id,p.name,CONCAT_WS(' / ',v.name,NULLIF(v.subvariant,'')),v.price,v.grams
FROM variants v JOIN products p ON p.id=v.product_id JOIN shops s ON s.id=p.shop_id JOIN users u ON u.id=s.owner_id
WHERE p.status='active' AND v.enabled AND NOT s.suspended
  AND u.email REGEXP '^owner[0-9]{2}@seed[.]tosca[.]invalid$';

CREATE TEMPORARY TABLE history_shops (
    slot INT PRIMARY KEY, shop_id BIGINT NOT NULL, owner_id BIGINT NOT NULL, variants INT NOT NULL
) ENGINE=InnoDB;
INSERT INTO history_shops
SELECT ROW_NUMBER() OVER (ORDER BY u.email,s.id), s.id, s.owner_id, v.slots
FROM history_variants v JOIN shops s ON s.id=v.shop_id JOIN users u ON u.id=s.owner_id
WHERE v.slot=1;

SET @history_buyer_count = (SELECT COUNT(*) FROM history_buyers);
SET @history_shop_count = (SELECT COUNT(*) FROM history_shops);

CREATE TEMPORARY TABLE history_checkout (
    n INT PRIMARY KEY, number CHAR(36) NOT NULL UNIQUE, buyer BIGINT NOT NULL, address VARCHAR(1100) NOT NULL,
    days_ago INT NOT NULL, created DATETIME NOT NULL, payment VARCHAR(12) NOT NULL
) ENGINE=InnoDB;
INSERT INTO history_checkout
SELECT x.seq,UUID(),b.user_id,b.address,x.days_ago,
       DATE(@seed_now)-INTERVAL x.days_ago DAY+INTERVAL (1+MOD(x.draw,15)) HOUR+INTERVAL MOD(x.draw,60) MINUTE,
       CASE WHEN MOD(x.draw,100)<4 THEN 'cancelled'
            WHEN x.days_ago<=3 AND MOD(x.draw,100)<20 THEN 'waiting' ELSE 'paid' END
FROM (
    SELECT seq, CRC32(CONCAT('checkout-',seq)) AS draw,
        CASE WHEN seq<=950 THEN 1+MOD(CRC32(CONCAT('day-',seq)),30)
             WHEN seq<=2350 THEN 31+MOD(CRC32(CONCAT('day-',seq)),60)
             WHEN seq<=5600 THEN 91+MOD(CRC32(CONCAT('day-',seq)),275)
             WHEN seq<=7700 THEN 366+MOD(CRC32(CONCAT('day-',seq)),535)
             ELSE 901+MOD(CRC32(CONCAT('day-',seq)),550) END AS days_ago
    FROM seq_1_to_8200
) x JOIN history_buyers b ON b.slot=1+MOD(CRC32(CONCAT('buyer-',x.seq)),@history_buyer_count);

CREATE TEMPORARY TABLE history_orders (
    n INT NOT NULL, seq INT NOT NULL, shop_id BIGINT NOT NULL, owner_id BIGINT NOT NULL,
    variants INT NOT NULL, status VARCHAR(20) NOT NULL, PRIMARY KEY (n,seq)
) ENGINE=InnoDB;
INSERT INTO history_orders
SELECT selected.n,g.seq,s.shop_id,s.owner_id,s.variants,
       CASE WHEN selected.payment='waiting' THEN 'waiting_payment'
            WHEN selected.payment='cancelled' THEN 'cancelled'
            WHEN MOD(selected.draw+g.seq,100)<3 THEN 'rejected'
            WHEN selected.days_ago<=2 THEN IF(MOD(selected.draw+g.seq,3)=0,'waiting_shop','packing')
            WHEN selected.days_ago<=5 THEN IF(MOD(selected.draw+g.seq,4)=0,'packing','delivery')
            ELSE 'completed' END
FROM (
    SELECT n,payment,days_ago,CRC32(CONCAT('split-',n)) AS draw,
        IF(MOD(CRC32(CONCAT('primary-',n)),100)<18,1,1+MOD(CRC32(CONCAT('shop-',n)),@history_shop_count)) AS main_slot
    FROM history_checkout
) selected
JOIN seq_1_to_2 g ON g.seq=1 OR (@history_shop_count>1 AND MOD(selected.draw,100)<28)
JOIN history_shops s ON s.slot=1+MOD(selected.main_slot-1+IF(g.seq=1,0,1+MOD(selected.draw,@history_shop_count-1)),@history_shop_count);

CREATE TEMPORARY TABLE history_lines (
    n INT NOT NULL, seq INT NOT NULL, variant_id BIGINT NOT NULL, product_id BIGINT NOT NULL,
    name VARCHAR(180) NOT NULL, variant_name VARCHAR(306) NOT NULL, price BIGINT NOT NULL,
    quantity INT NOT NULL, grams INT NOT NULL, PRIMARY KEY(n,seq,variant_id)
) ENGINE=InnoDB;
INSERT INTO history_lines
SELECT o.n,o.seq,v.variant_id,v.product_id,v.product_name,v.variant_name,
       GREATEST(1,ROUND(v.price * IF(MOD(o.n+o.seq,13)=0,.9,1))),
       CASE WHEN MOD(CRC32(CONCAT('quantity-',o.n,'-',l.seq)),100)<77 THEN 1
            ELSE 2+MOD(o.n+l.seq,3) END,v.grams
FROM history_orders o
JOIN seq_1_to_3 l ON l.seq<=LEAST(o.variants,1+IF(MOD(o.n+o.seq,5)<2,1,0)+IF(MOD(o.n,19)=0,1,0))
JOIN history_variants v ON v.shop_id=o.shop_id
  AND v.slot=1+MOD(CRC32(CONCAT('item-',o.n,'-',o.seq))+l.seq-1,o.variants);

CREATE TEMPORARY TABLE history_totals (
    n INT NOT NULL, seq INT NOT NULL, subtotal BIGINT NOT NULL, shipping BIGINT NOT NULL, tax BIGINT NOT NULL,
    PRIMARY KEY(n,seq)
) ENGINE=InnoDB;
INSERT INTO history_totals
SELECT n,seq,SUM(price*quantity),SUM(grams*quantity),FLOOR((SUM(price*quantity)*@seed_tax+5000)/10000)
FROM history_lines GROUP BY n,seq;

INSERT INTO checkouts(checkout_no,user_id,address,subtotal,tax,shipping,total,payment,paid_by,created_at,updated_at,stock_reserved)
SELECT c.number,c.buyer,c.address,t.subtotal,t.tax,t.shipping,t.subtotal+t.tax+t.shipping,c.payment,
       IF(c.payment='paid',@fixture_admin,NULL),c.created,c.created,FALSE
FROM history_checkout c
JOIN (SELECT n,SUM(subtotal) subtotal,SUM(shipping) shipping,SUM(tax) tax FROM history_totals GROUP BY n) t ON t.n=c.n
ORDER BY c.created,c.n;
SET @history_inserted = ROW_COUNT();

INSERT INTO orders(checkout_id,shop_id,shipping,subtotal,tax,status,courier_id,receipt,completed_at,actor_id,created_at,updated_at)
SELECT db.id,o.shop_id,t.shipping,t.subtotal,t.tax,o.status,
       IF(o.status IN ('delivery','completed'),1,NULL),
       IF(o.status IN ('delivery','completed'),CONCAT('HIST-',LPAD(o.n,5,'0'),'-',o.seq),''),
       IF(o.status='completed',c.created+INTERVAL (22+MOD(o.n*13+o.seq*7,93)) HOUR,NULL),
       CASE WHEN o.status IN ('cancelled','waiting_payment','completed') THEN c.buyer
            WHEN o.status='waiting_shop' THEN @fixture_admin ELSE o.owner_id END,
       c.created,CASE WHEN o.status='completed' THEN c.created+INTERVAL (22+MOD(o.n*13+o.seq*7,93)) HOUR
           WHEN o.status='waiting_payment' THEN c.created ELSE LEAST(@seed_now,c.created+INTERVAL 16 HOUR) END
FROM history_orders o JOIN history_checkout c ON c.n=o.n JOIN checkouts db ON db.checkout_no=c.number
JOIN history_totals t ON t.n=o.n AND t.seq=o.seq
ORDER BY c.created,o.n,o.seq;

INSERT INTO order_lines(order_id,variant_id,product_id,product_name,variant_name,price,quantity,grams,created_at,updated_at)
SELECT db_order.id,l.variant_id,l.product_id,l.name,l.variant_name,l.price,l.quantity,l.grams,c.created,c.created
FROM history_lines l JOIN history_checkout c ON c.n=l.n JOIN checkouts db ON db.checkout_no=c.number
JOIN history_orders o ON o.n=l.n AND o.seq=l.seq
JOIN orders db_order ON db_order.checkout_id=db.id AND db_order.shop_id=o.shop_id;

INSERT INTO tracking(order_id,note,actor_id,created_at,updated_at)
SELECT db_order.id,ELT(tracking_step.seq,'Parcel handed to courier','Arrived at the local hub','Package delivered'),
       o.owner_id,IF(tracking_step.seq=3,db_order.completed_at,db_order.created_at+INTERVAL (6+tracking_step.seq*5) HOUR),
       IF(tracking_step.seq=3,db_order.completed_at,db_order.created_at+INTERVAL (6+tracking_step.seq*5) HOUR)
FROM history_orders o JOIN history_checkout c ON c.n=o.n JOIN checkouts db ON db.checkout_no=c.number
JOIN orders db_order ON db_order.checkout_id=db.id AND db_order.shop_id=o.shop_id
JOIN seq_1_to_3 tracking_step ON tracking_step.seq<3 OR db_order.completed_at IS NOT NULL
WHERE o.status IN ('completed','delivery');

INSERT INTO sessions(id,user_id,refresh_hash,expires_at,revoked,created_at,updated_at)
SELECT UUID(),buyer,REPEAT('0',64),created+INTERVAL 30 DAY,TRUE,created,created
FROM history_checkout WHERE MOD(n,5)<>0;

-- Only replace photos from the original fixtures. Uploaded product images have different paths.
UPDATE products p JOIN shops s ON s.id=p.shop_id JOIN users u ON u.id=s.owner_id
SET p.image=''
WHERE u.email REGEXP '^owner[0-9]{2}@seed[.]tosca[.]invalid$'
  AND p.image IN ('/catalog/sneakers.jpg','/catalog/mug.jpg','/catalog/headphones.jpg','/catalog/bag.jpg',
                  '/catalog/lamp.jpg','/catalog/beauty.jpg','/catalog/running.jpg','/catalog/coffee.jpg');

DROP TEMPORARY TABLE history_totals,history_lines,history_orders,history_checkout,history_shops,history_variants,history_buyers;
