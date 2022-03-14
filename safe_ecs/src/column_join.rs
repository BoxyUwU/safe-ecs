use std::cell::{Ref, RefMut};

use crate::static_columns::StaticColumnsInner;
use crate::world::Archetype;
use crate::{Component, EcsTypeId, Entity, World};
use crate::{StaticColumns, WorldId};

pub struct ColumnIterator<'lock, 'world: 'lock, C: Joinable + 'lock> {
    archetype_iter: ArchetypeIter<'world, 'lock, C>,
    state: C::State<'lock>,
    column_iter: Option<C::ItemIter<'lock>>,
}

pub struct ColumnLocks<'world_data, 'world, C: Joinable + 'world> {
    ids: C::Ids,
    world: &'world World<'world_data>,
    locks: C::Locks<'world>,
}

type ArchetypeIter<'world: 'lock, 'lock, C: Joinable + 'lock> =
    impl Iterator<Item = &'world Archetype> + 'lock;
impl<'world_data, 'world, C: Joinable + 'world> ColumnLocks<'world_data, 'world, C> {
    pub fn new(borrows: C, world: &'world World<'world_data>) -> Self {
        C::assert_world_id(&borrows, world.id);
        let ids = C::make_ids(&borrows, world);
        let locks = C::make_locks(borrows, world);
        Self { ids, locks, world }
    }
}

impl<'lock, 'world_data, 'world, C: Joinable> ColumnIterator<'lock, 'world, C> {
    pub fn new(locks: &'lock mut ColumnLocks<'world_data, 'world, C>) -> Self {
        let state = C::make_state(&mut locks.locks);

        fn defining_use<'world: 'lock, 'lock, C: Joinable + 'lock>(
            world: &'world World,
            ids: C::Ids,
        ) -> ArchetypeIter<'world, 'lock, C> {
            world
                .archetypes
                .iter()
                .filter(move |archetype| C::archetype_matches(&ids, archetype))
        }
        ColumnIterator {
            archetype_iter: defining_use(locks.world, locks.ids),
            state,
            column_iter: None,
        }
    }
}

impl<'lock, 'world: 'lock, C: Joinable + 'lock> Iterator for ColumnIterator<'lock, 'world, C> {
    type Item = C::Item<'lock>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let Some(iter) = &mut self.column_iter else {
                let archetype = self.archetype_iter.next()?;
                let iter = C::iter_from_archetype(&mut self.state, archetype);
                self.column_iter = Some(iter);
                continue;
            };

            let Some(v) = C::advance_iter(iter) else {
                self.column_iter = None;
                continue;
            };
            return Some(v);
        }
    }
}

impl<'lock, 'world_data, 'world: 'lock, C: Joinable + 'lock> IntoIterator
    for &'lock mut ColumnLocks<'world_data, 'world, C>
{
    type Item = C::Item<'lock>;
    type IntoIter = ColumnIterator<'lock, 'world, C>;

    fn into_iter(self) -> Self::IntoIter {
        ColumnIterator::new(self)
    }
}

//~ joinable impls

/// This trait is also implemented for tuples up to length 8 where all elements implement this trait
pub trait Joinable {
    type Ids: Copy;

    type Locks<'world>
    where
        Self: 'world;

    type State<'lock>
    where
        Self: 'lock;

    type Item<'lock>
    where
        Self: 'lock;

    type ItemIter<'lock>
    where
        Self: 'lock;

    fn make_ids(&self, world: &World) -> Self::Ids;
    fn make_locks<'world>(self, world: &'world World) -> Self::Locks<'world>
    where
        Self: 'world;
    fn make_state<'lock, 'world>(locks: &'lock mut Self::Locks<'world>) -> Self::State<'lock>
    where
        Self: 'lock + 'world;
    fn iter_from_archetype<'world>(
        state: &mut Self::State<'world>,
        archetype: &'world Archetype,
    ) -> Self::ItemIter<'world>
    where
        Self: 'world;
    fn archetype_matches(ids: &Self::Ids, archetype: &Archetype) -> bool;
    fn advance_iter<'world>(iter: &mut Self::ItemIter<'world>) -> Option<Self::Item<'world>>
    where
        Self: 'world;
    fn assert_world_id(&self, world_id: WorldId);
}

impl<'a, T: Component> Joinable for &'a StaticColumns<T> {
    type Ids = EcsTypeId;
    type Locks<'world>
    where
        Self: 'world,
    = (EcsTypeId, Ref<'world, StaticColumnsInner<T>>);

    type State<'lock>
    where
        Self: 'lock,
    = (EcsTypeId, &'lock [Vec<T>]);

    type Item<'lock>
    where
        Self: 'lock,
    = &'lock T;

    type ItemIter<'lock>
    where
        Self: 'lock,
    = std::slice::Iter<'lock, T>;

    fn make_ids(&self, _: &World) -> Self::Ids {
        self.id
    }

    fn make_locks<'world>(self, _: &'world World) -> Self::Locks<'world>
    where
        Self: 'world,
    {
        (self.id, self.inner.borrow())
    }

    fn make_state<'lock, 'world>(locks: &'lock mut Self::Locks<'world>) -> Self::State<'lock>
    where
        Self: 'world + 'lock,
    {
        (locks.0, &locks.1 .0[..])
    }

    fn iter_from_archetype<'world>(
        (id, state): &mut (EcsTypeId, &'world [Vec<T>]),
        archetype: &'world Archetype,
    ) -> Self::ItemIter<'world>
    where
        Self: 'world,
    {
        let col = archetype.column_indices[id];
        let foo = *state;
        foo[col].iter()
    }

    fn archetype_matches(ids: &Self::Ids, archetype: &Archetype) -> bool {
        archetype.column_indices.contains_key(ids)
    }

    fn advance_iter<'world>(iter: &mut Self::ItemIter<'world>) -> Option<Self::Item<'world>>
    where
        Self: 'world,
    {
        iter.next()
    }

    fn assert_world_id(&self, world_id: WorldId) {
        (**self).assert_world_id(world_id);
    }
}

impl<'a, T: Component> Joinable for &'a mut StaticColumns<T> {
    type Ids = EcsTypeId;
    type Locks<'world>
    where
        Self: 'world,
    = (EcsTypeId, RefMut<'world, StaticColumnsInner<T>>);

    type State<'lock>
    where
        Self: 'lock,
    = (EcsTypeId, usize, &'lock mut [Vec<T>]);

    type Item<'lock>
    where
        Self: 'lock,
    = &'lock mut T;

    type ItemIter<'lock>
    where
        Self: 'lock,
    = std::slice::IterMut<'lock, T>;

    fn make_ids(&self, _: &World) -> Self::Ids {
        self.id
    }

    fn make_locks<'world>(self, _: &'world World) -> Self::Locks<'world>
    where
        Self: 'world,
    {
        (self.id, self.inner.borrow_mut())
    }

    fn make_state<'lock, 'world>(locks: &'lock mut Self::Locks<'world>) -> Self::State<'lock>
    where
        Self: 'world + 'lock,
    {
        (locks.0, 0, &mut locks.1 .0[..])
    }

    fn archetype_matches(ids: &Self::Ids, archetype: &Archetype) -> bool {
        archetype.column_indices.contains_key(ids)
    }

    fn advance_iter<'world>(iter: &mut Self::ItemIter<'world>) -> Option<Self::Item<'world>>
    where
        Self: 'world,
    {
        iter.next()
    }

    fn iter_from_archetype<'world>(
        (ecs_type_id, num_chopped_off, lock_borrow): &mut Self::State<'world>,
        archetype: &'world Archetype,
    ) -> Self::ItemIter<'world>
    where
        Self: 'world,
    {
        let col = archetype.column_indices[ecs_type_id];
        assert!(col >= *num_chopped_off);
        let idx = col - *num_chopped_off;
        let taken_out_borrow = std::mem::replace(lock_borrow, &mut []);
        let (chopped_of, remaining) = taken_out_borrow.split_at_mut(idx + 1);
        *lock_borrow = remaining;
        *num_chopped_off += chopped_of.len();
        chopped_of.last_mut().unwrap().iter_mut()
    }

    fn assert_world_id(&self, world_id: WorldId) {
        (**self).assert_world_id(world_id);
    }
}

pub struct WithEntities;
impl Joinable for WithEntities {
    type Ids = ();

    type Locks<'world>
    where
        Self: 'world,
    = ();

    type State<'lock>
    where
        Self: 'lock,
    = ();

    type Item<'lock>
    where
        Self: 'lock,
    = Entity;

    type ItemIter<'lock>
    where
        Self: 'lock,
    = std::slice::Iter<'lock, Entity>;

    fn make_ids(&self, _: &World) -> Self::Ids {}

    fn make_locks<'world>(self, _: &'world World) -> Self::Locks<'world>
    where
        Self: 'world,
    {
    }

    fn make_state<'lock, 'world>(_: &'lock mut Self::Locks<'world>) -> Self::State<'lock>
    where
        Self: 'lock + 'world,
    {
    }

    fn iter_from_archetype<'world>(
        _: &mut Self::State<'world>,
        archetype: &'world Archetype,
    ) -> Self::ItemIter<'world>
    where
        Self: 'world,
    {
        archetype.entities.iter()
    }

    fn archetype_matches(_: &Self::Ids, _: &Archetype) -> bool {
        true
    }

    fn advance_iter<'world>(iter: &mut Self::ItemIter<'world>) -> Option<Self::Item<'world>>
    where
        Self: 'world,
    {
        iter.next().copied()
    }

    fn assert_world_id(&self, _: WorldId) {}
}

pub struct Maybe<J: Joinable>(pub J);
pub enum Either<T, U> {
    T(T),
    U(U),
}
impl<J: Joinable> Joinable for Maybe<J> {
    type Ids = ();

    type Locks<'world>
    where
        Self: 'world,
    = (J::Ids, J::Locks<'world>);

    type State<'lock>
    where
        Self: 'lock,
    = (J::Ids, J::State<'lock>);

    type Item<'lock>
    where
        Self: 'lock,
    = Option<J::Item<'lock>>;

    type ItemIter<'lock>
    where
        Self: 'lock,
    = Either<J::ItemIter<'lock>, std::ops::Range<usize>>;

    fn make_ids(&self, _: &World) -> Self::Ids {}

    fn make_locks<'world>(self, world: &'world World) -> Self::Locks<'world>
    where
        Self: 'world,
    {
        (J::make_ids(&self.0, world), J::make_locks(self.0, world))
    }

    fn make_state<'lock, 'world>(locks: &'lock mut Self::Locks<'world>) -> Self::State<'lock>
    where
        Self: 'lock + 'world,
    {
        (locks.0, J::make_state(&mut locks.1))
    }

    fn iter_from_archetype<'world>(
        (ids, state): &mut Self::State<'world>,
        archetype: &'world Archetype,
    ) -> Self::ItemIter<'world>
    where
        Self: 'world,
    {
        match J::archetype_matches(ids, archetype) {
            true => Either::T(J::iter_from_archetype(state, archetype)),
            false => Either::U(0..archetype.entities.len()),
        }
    }

    fn archetype_matches(_: &Self::Ids, _: &Archetype) -> bool {
        true
    }

    fn advance_iter<'world>(iter: &mut Self::ItemIter<'world>) -> Option<Self::Item<'world>>
    where
        Self: 'world,
    {
        match iter {
            Either::T(t) => J::advance_iter(t).map(Some),
            Either::U(u) => u.next().map(|_| None),
        }
    }

    fn assert_world_id(&self, world_id: WorldId) {
        J::assert_world_id(&self.0, world_id)
    }
}

pub struct Unsatisfied<J: Joinable>(pub J);
impl<J: Joinable> Joinable for Unsatisfied<J> {
    type Ids = J::Ids;

    type Locks<'world>
    where
        Self: 'world,
    = J::Locks<'world>;

    type State<'lock>
    where
        Self: 'lock,
    = J::State<'lock>;

    type Item<'lock>
    where
        Self: 'lock,
    = ();

    type ItemIter<'lock>
    where
        Self: 'lock,
    = std::ops::Range<usize>;

    fn make_ids(&self, world: &World) -> Self::Ids {
        J::make_ids(&self.0, world)
    }

    fn make_locks<'world>(self, world: &'world World) -> Self::Locks<'world>
    where
        Self: 'world,
    {
        J::make_locks(self.0, world)
    }

    fn make_state<'lock, 'world>(locks: &'lock mut Self::Locks<'world>) -> Self::State<'lock>
    where
        Self: 'lock + 'world,
    {
        J::make_state(locks)
    }

    fn iter_from_archetype<'world>(
        _: &mut Self::State<'world>,
        archetype: &'world Archetype,
    ) -> Self::ItemIter<'world>
    where
        Self: 'world,
    {
        0..archetype.entities.len()
    }

    fn archetype_matches(ids: &Self::Ids, archetype: &Archetype) -> bool {
        J::archetype_matches(ids, archetype) == false
    }

    fn advance_iter<'world>(iter: &mut Self::ItemIter<'world>) -> Option<Self::Item<'world>>
    where
        Self: 'world,
    {
        iter.next().map(|_| ())
    }

    fn assert_world_id(&self, world_id: WorldId) {
        J::assert_world_id(&self.0, world_id)
    }
}

macro_rules! tuple_impls_joinable {
    ($($T:ident)*) => {
        #[doc(hidden)]
        #[allow(unused_parens)]
        #[allow(non_snake_case)]
        impl<$($T: Joinable),*> Joinable for ($($T,)*) {
            type Ids = ($($T::Ids,)*);

            type Locks<'world>
            where
                Self: 'world = ($($T::Locks<'world>,)*);

            type State<'lock>
            where
                Self: 'lock = ($($T::State<'lock>,)*);

            type Item<'lock>
            where
                Self: 'lock = ($($T::Item<'lock>,)*);

            type ItemIter<'lock>
            where
                Self: 'lock = ($($T::ItemIter<'lock>,)*);

            fn make_ids(&self, world: &World) -> Self::Ids {
                let ($($T,)*) = self;
                ($($T::make_ids($T, world),)*)
            }
            fn make_locks<'world>(self, world: &'world World) -> Self::Locks<'world>
            where
                Self: 'world {
                    let ($($T,)*) = self;
                    ($($T::make_locks($T, world),)*)
                }
            fn make_state<'lock, 'world>(locks: &'lock mut Self::Locks<'world>) -> Self::State<'lock>
            where
                Self: 'lock + 'world {
                    let ($($T,)*) = locks;
                    ($($T::make_state($T),)*)
                }
            fn iter_from_archetype<'world>(
                state: &mut Self::State<'world>,
                archetype: &'world Archetype,
            ) -> Self::ItemIter<'world>
            where
                Self: 'world {
                    let ($($T,)*) = state;
                    ($($T::iter_from_archetype($T, archetype),)*)
                }
            fn archetype_matches(ids: &Self::Ids, archetype: &Archetype) -> bool {
                let ($($T,)*) = ids;
                true $(&& $T::archetype_matches($T, archetype))*
            }
            fn advance_iter<'world>(iter: &mut Self::ItemIter<'world>) -> Option<Self::Item<'world>>
            where
                Self: 'world {
                    let ($($T,)*) = iter;
                    Some(($($T::advance_iter($T)?,)*))
                }
            fn assert_world_id(&self, world_id: WorldId) {
                let ($($T,)*) = self;
                $($T::assert_world_id($T, world_id);)*
            }
        }
    };
}

tuple_impls_joinable!(A B C D E F G H);
tuple_impls_joinable!(A B C D E F G);
tuple_impls_joinable!(A B C D E F);
tuple_impls_joinable!(A B C D E);
tuple_impls_joinable!(A B C D);
tuple_impls_joinable!(A B C);
tuple_impls_joinable!(A B);
tuple_impls_joinable!(A);

#[cfg(test)]
mod tests {
    use crate::*;

    #[test]
    fn for_loop() {
        let mut world = World::new();
        let mut u32s = world.new_static_column::<u32>();
        let e1 = world.spawn().insert(&mut u32s, 10).id();
        for data in ColumnLocks::new((WithEntities, &u32s), &world).into_iter() {
            assert_eq!(data, (e1, &10));
            return;
        }
        unreachable!()
    }

    #[test]
    fn simple_query() {
        let mut world = World::new();
        let mut u32s = world.new_static_column::<u32>();
        let mut u64s = world.new_static_column::<u64>();
        let mut u128s = world.new_static_column::<u128>();
        world
            .spawn()
            .insert(&mut u32s, 10_u32)
            .insert(&mut u64s, 12_u64)
            .id();
        world
            .spawn()
            .insert(&mut u64s, 13_u64)
            .insert(&mut u128s, 9_u128)
            .id();
        let mut locks = ColumnLocks::new(&u64s, &world);
        let returned = locks.into_iter().collect::<Vec<_>>();
        assert_eq!(returned, [&12, &13]);
    }

    #[test]
    fn tuple_query() {
        let mut world = World::new();
        let mut u32s = world.new_static_column::<u32>();
        let mut u64s = world.new_static_column::<u64>();
        let mut u128s = world.new_static_column::<u128>();
        let e1 = world
            .spawn()
            .insert(&mut u32s, 10_u32)
            .insert(&mut u64s, 12_u64)
            .id();
        world
            .spawn()
            .insert(&mut u64s, 13_u64)
            .insert(&mut u128s, 9_u128)
            .id();
        let mut locks = ColumnLocks::new((WithEntities, &u32s, &u64s), &world);
        let returned = locks.into_iter().collect::<Vec<_>>();
        assert_eq!(returned, [(e1, &10, &12)]);
    }

    #[test]
    fn maybe_query() {
        let mut world = World::new();
        let mut u32s = world.new_static_column::<u32>();
        let mut u64s = world.new_static_column::<u64>();
        let mut u128s = world.new_static_column::<u128>();

        let e1 = world
            .spawn()
            .insert(&mut u32s, 10_u32)
            .insert(&mut u64s, 12_u64)
            .id();
        let e2 = world
            .spawn()
            .insert(&mut u64s, 13_u64)
            .insert(&mut u128s, 9_u128)
            .id();

        let mut locks =
            ColumnLocks::new((WithEntities, Maybe(&u32s), &u64s, Maybe(&u128s)), &world);
        let returned = locks.into_iter().collect::<Vec<_>>();
        assert_eq!(
            returned,
            [
                (e1, Some(&10_u32), &12_u64, None),
                (e2, None, &13_u64, Some(&9_u128))
            ],
        )
    }

    #[test]
    fn query_with_despawned() {
        let mut world = World::new();
        let mut u32s = world.new_static_column::<u32>();
        let e1 = world.spawn().insert(&mut u32s, 10_u32).id();
        world.despawn(e1);

        let mut locks = ColumnLocks::new(&u32s, &world);
        let mut iter = locks.into_iter();
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn complex_maybe_query() {
        let mut world = World::new();
        let mut u32s = world.new_static_column::<u32>();
        let u64s = world.new_static_column::<u64>();
        let e1 = world.spawn().insert(&mut u32s, 10_u32).id();
        let e2 = world.spawn().insert(&mut u32s, 12_u32).id();
        let mut locks = ColumnLocks::new((WithEntities, Maybe(&u64s), &u32s), &world);
        let returned = locks.into_iter().collect::<Vec<_>>();
        assert_eq!(returned, [(e1, None, &10_u32), (e2, None, &12_u32)]);
    }
}

#[cfg(test)]
mod mismatched_world_id_tests {
    use crate::*;

    #[test]
    #[should_panic = "[Mismatched WorldIds]:"]
    fn ref_join() {
        let world = World::new();
        let mut other_world = World::new();
        let other_u32s = other_world.new_static_column::<u32>();
        ColumnLocks::new(&other_u32s, &world);
    }

    #[test]
    #[should_panic = "[Mismatched WorldIds]:"]
    fn mut_join() {
        let world = World::new();
        let mut other_world = World::new();
        let mut other_u32s = other_world.new_static_column::<u32>();
        ColumnLocks::new(&mut other_u32s, &world);
    }

    #[test]
    #[should_panic = "[Mismatched WorldIds]:"]
    fn maybe_join() {
        let world = World::new();
        let mut other_world = World::new();
        let other_u32s = other_world.new_static_column::<u32>();
        ColumnLocks::new(Maybe(&other_u32s), &world);
    }

    #[test]
    #[should_panic = "[Mismatched WorldIds]:"]
    fn unsatisfied_join() {
        let world = World::new();
        let mut other_world = World::new();
        let other_u32s = other_world.new_static_column::<u32>();
        ColumnLocks::new(Unsatisfied(&other_u32s), &world);
    }

    #[test]
    #[should_panic = "[Mismatched WorldIds]:"]
    fn multi_join() {
        let world = World::new();
        let mut other_world = World::new();
        let other_u32s = other_world.new_static_column::<u32>();
        ColumnLocks::new((WithEntities, &other_u32s), &world);
    }
}
